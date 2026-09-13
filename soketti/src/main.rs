mod virtual_socket;
use std::{net::SocketAddr, sync::Arc, thread, time::Duration};
use std::sync::Mutex;
use virtual_socket::VirtualSocket;
use std::{io::{self, Write}};

const RECEIVER_ADDR: &str = "127.0.0.1:8080";
const OWN_ADDR: &str = "127.0.0.1:7878";

// Stores all rdt versions
pub enum RdtVersion {
    AckNack,
    AckOnly,   
    NackOnly,
    FullRdt,
    GbnReceiver,
    GbnSender,
}

fn main() -> std::io::Result<()> {
    {
        let mut socket = VirtualSocket::new(OWN_ADDR)?;
        let rdt_version = get_rdt_version_from_user();
        let mut latest_sequence = 0x01; //Assume latest sequence number is 1 
        let mut gbn_latest_seq = 0xFE; // Since we start indexing at 0 we need to init. this as a
                                       // high number
        let mut gbn_next_seq = 0x00;
        match rdt_version {
            // rdt 3.0 is identical to 2.2 on the receiver end, so we need a totally different
            // functionality
            
            RdtVersion::FullRdt => {full_rdt(&mut socket)} 
            RdtVersion::GbnSender => {gbn_sender(&mut socket)}
            _ => {} 
        }
        loop {
            println!("Listening...");
            let mut buffer = [0; 256];
            let (am, addr) = socket.recv_from(&mut buffer).expect("Data not received");
        
            let filled_buf = &mut buffer[..am-1];
            let msg = String::from_utf8_lossy(filled_buf);
            println!("Received {} bytes from {}: {:?}", am, addr, msg);
            println!("Last bit: 0x{:02x}", buffer[am-1]);
            let checksum = check_for_bit_errors(&buffer[..am]);
            let success = checksum == 0;  
            if !success {
                println!("Bit error in message! Checksum was {}", checksum);
            }
            match rdt_version {
                
                // Flow with both ACKs and NACKs
                RdtVersion::AckNack => {
                    let ack_packet = hanlde_ack_or_nack(success);
                    match socket.send_ack(addr, &ack_packet) {
                        Ok(_) => println!("Sent {} to {addr}", String::from_utf8_lossy(&ack_packet)),
                        Err(e) => eprintln!("Error {e}")
                    }

                }

                // Flow with only ACKs 
                RdtVersion::AckOnly => {
                    let (ack_packet, seq) = handle_only_ack(success, &buffer, latest_sequence);
                    latest_sequence = seq;

                    match socket.send_ack(addr, &ack_packet) {
                        Ok(_) => println!("Sent {} to {addr}", String::from_utf8_lossy(&ack_packet)),
                        Err(e) => eprintln!("Error {e}")
                    }
                }

                // Flow with only NACKs 
                RdtVersion::NackOnly => {
                    match handle_only_nack(success) {
                        Some(val) => match socket.send_ack(addr, &val) {
                            Ok(_) => println!("Sent {} to {addr}", String::from_utf8_lossy(&val)),
                            Err(e) => eprintln!("Error {e}")
                        }
                        None => println!("No errors so not sending anything")
                    }
                }

                // Go back N receiver similar to RDT 2.2 except Seq numbers are not limited to 0
                // and 1 
                RdtVersion::GbnReceiver => {
                    let (ack_packet, seq, next_seq) = gbn_handle_recv(success, &buffer, gbn_latest_seq, gbn_next_seq);
                    gbn_latest_seq = seq;
                    gbn_next_seq = next_seq;

                    match socket.send_ack(addr, &ack_packet) {
                        Ok(_) => println!("Sent {} to {addr}", String::from_utf8_lossy(&ack_packet)),
                        Err(e) => eprintln!("Error {e}")
                    }

                }

                _ => {}
            }
        }
    }
}

fn gbn_handle_recv(success: bool, message: &[u8], latest_seq_num: u8, next_seq_num: u8) -> ([u8; 5], u8, u8) {
    /* 
     * GBN receiver. 
     * */
    if success {
        // Extract sequence number from packet
        let sequence_number = message[0];
        
        if sequence_number == next_seq_num {
            let buff: &[u8] = b"ACK";
            let mut ack_packet = [0u8; 5];
            ack_packet[1..4].copy_from_slice(buff);
        
            // Put sequence number as the first byte and CRC8 as the last 
            ack_packet[0] = sequence_number;
            let crc8 = check_for_bit_errors(&ack_packet);
            ack_packet[4] = crc8;
            return (ack_packet, next_seq_num, next_seq_num + 0x01);

        }
    } 
    
    // Packet was not received properly send ACK of the previous packet 
    let buff: &[u8] = b"ACK";
    let mut ack_packet = [0u8; 5];
    ack_packet[1..4].copy_from_slice(buff);
    ack_packet[0] = latest_seq_num;
    let crc8 = check_for_bit_errors(&ack_packet);
    ack_packet[4] = crc8;
    
    // Also return the latest sequence here
    return (ack_packet, latest_seq_num, next_seq_num);

}

pub struct WindowState {
    pub current_seq_num: u8,
    pub next_seq_num: u8,
    pub window_size: u8,
    pub unacked_packets: Vec<Vec<u8>>,
}

fn gbn_sender(socket: &mut VirtualSocket) {
    /* 
     * Go Back N sender client. separated sender and receiver for clarity
     * */
    
    // Setup Window state
    // 5 messages at a time for now
    let window_size: usize = 5;
    let state = WindowState{
        current_seq_num: 0,
        next_seq_num: 0,
        window_size: window_size as u8,
        unacked_packets: Vec::with_capacity(window_size),
    };

    let mutex_state = Arc::new(Mutex::new(state));
    let cloned_mutex_state = mutex_state.clone();
    // Setup listener   
    let mut listener_socket = socket.try_clone();    
    // Set listening to another thread
    thread::spawn(move || {
        println!("Starting listener socket...");

        // Set timeout
        listener_socket.set_timer(Duration::from_secs(20));
        
        loop {

            // Receive from socket 
            let mut buffer = [0; 256];
            match listener_socket.recv_from(&mut buffer) {
                Ok((am, addr)) => {
                    let filled_buf = &mut buffer.clone()[..am-1];
                    let msg = String::from_utf8_lossy(filled_buf);
                    println!("Received {} bytes from {}: {:?}", am, addr, msg);
                    println!("Last bit: 0x{:02x}", buffer[am-1]);
            
                    // Checksum 
                    let checksum = check_for_bit_errors(&buffer[..am]);
                    let success = checksum == 0;  
                    if success {
                        let mut curr_state = cloned_mutex_state.lock().expect("Thread Poisoned!");
                
                        // Get Seq num from ACK 
                        let seq_num = filled_buf[0];

                        // Only accept current seq num or higher, since 
                        if seq_num >= curr_state.current_seq_num &&
                            seq_num < (curr_state.window_size + curr_state.current_seq_num){

                            // increment necessary data only if seq_num matches 
                            curr_state.current_seq_num = seq_num + 0x01;
                            curr_state.unacked_packets.retain(|packet| packet[0] > seq_num);
                        }

                    } else {
                        println!("Bit error in message! Checksum was {}", checksum);

                    } 

                }

                Err(_) => {
                    // Logic for resending packets
                    println!("Timer ran out, resending packets!");

                    {
                        let curr_state = cloned_mutex_state.lock().expect("Thread poisoned!");
                        let receiver_addr: SocketAddr = RECEIVER_ADDR.parse().expect("Failed to parse");    

                        for unack_packet in &curr_state.unacked_packets {
                            match listener_socket.send_msg(&unack_packet, receiver_addr) {
                                Ok(am) => println!("Wrote {am} bytes"),
                                Err(e) => eprintln!("Error: {e}")
                            }
                        } 
                    }
                }
            }
        
        }
        
    });

    // Setup sending 
    let receiver_addr: SocketAddr = RECEIVER_ADDR.parse().expect("Failed to parse");    
    
    loop {
        print!("Input message>");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        
        // Read the user's input
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        input = String::from(input.trim_end());
        // Get bytes
        let input_bytes = input.as_bytes();

        // Create buffer variable here since it needs to live longer
        let mut buffer = Vec::with_capacity(1 + input_bytes.len() + 1);
        
        {
            let mut curr_state = mutex_state.lock().expect("Cannot lock something went wrong");

            // Only send when we have space. not very user intuitive but will do for now
            if (curr_state.next_seq_num as usize) < 
                (curr_state.current_seq_num as usize) + (curr_state.window_size as usize) {
                
                // Prepare buffer with data, sequence and crc8 
                buffer.push(curr_state.next_seq_num);
                buffer.extend_from_slice(input_bytes);
                buffer.push(0x00); // Need to push 0 byte here to get enough length for the buffer
                let crc8 = check_for_bit_errors(&buffer);
                *buffer.last_mut().unwrap() = crc8; // This can panic but we want the software to crash
        

                //Increment necessary data:
                curr_state.next_seq_num += 0x01;
                curr_state.unacked_packets.push(buffer.clone());
            } else {
                println!("Window full please retype the message");
                continue;
            }
        } // curr_state gets dropped here so mutex gets unlocked        
        
        // Send via virtual socket. Sending is a network operation so it shouldnt lock the mutex 
        match socket.send_msg(&buffer, receiver_addr) {
            Ok(am) => println!("Wrote {am} bytes"),
            Err(e) => eprintln!("Error: {e}")
        }
    }

}

fn full_rdt(socket: &mut VirtualSocket) {
    /* 
     * Full rdt 3.0 with message sending etc.
     * */

    // Set up 
    let receiver_addr: SocketAddr = "127.0.0.1:36516".parse().expect("Failed to parse");    
    socket.set_timer(Duration::from_secs(5));

    let mut latest_sequence = 0x00;
    let mut next_sequence = 0x01;
    loop {
        print!("Input message>");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        
        // Read the user's input
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        input = String::from(input.trim_end());

        // Get bytes
        let input_bytes = input.as_bytes();

        // Create buffer with seq and crc8
        let mut buffer = Vec::with_capacity(1 + input_bytes.len() + 1);
        buffer.push(latest_sequence);
        buffer.extend_from_slice(input_bytes);
        buffer.push(0x00); // Need to push 0 byte here to get enough length for the buffer
        let crc8 = check_for_bit_errors(&buffer);
        *buffer.last_mut().unwrap() = crc8; // This can panic but we want the software to crash
                                            // then. No need to build safety here.

        let mut send_success = false;

        // Repeat this until seding is successful
        while !send_success {

            // Send via virtual socket 
            match socket.send_msg(&buffer, receiver_addr) {
                Ok(am) => println!("Wrote {am} bytes"),
                Err(e) => eprintln!("Error: {e}")
            }

            // Similar to what we did before 
            let mut buffer = [0; 256];
            match socket.recv_from(&mut buffer) {
                Ok((am ,addr)) => {
                    let filled_buf = &mut buffer.clone()[..am-1]; // Need to clone buffer here
                                                                  // since it gets passed around
                                                                  // to many functions
                    let msg = String::from_utf8_lossy(filled_buf);
                    let checksum = check_for_bit_errors(&buffer[..am]); // Immutable borrow of
                                                                        // buffer
                    let success = checksum == 0;  
                    if !success {
                        println!("Received: {msg}. Bit error detected! Checksum was {}, resending message...", checksum);  // This print mutable borrows buffer so we needed to clone it 
 
                    }
                    else {
                        println!("Received {} bytes from {}: {:?}", am, addr, msg);
                        
                        // Verify ACK Packet 
                        if filled_buf[0] == latest_sequence {
                            println!("ACK has correct seq, success!");
                            send_success = true
                        }
                        else {
                            println!("ACK has incorrect seq, resending message...")
                        }
                    }
                    
                },
                
                // Techincally a network error causes this arm to trigger also but resending in that case is
                // also valid. println could be better tho...
                Err(_) => println!("Timer expired, resending message...")

            }         
            
        }

        // Swap latest and next
        let helper = latest_sequence;
        latest_sequence = next_sequence;
        next_sequence = helper;

    }
}

fn get_rdt_version_from_user() -> RdtVersion {
    /*
     * Interactive Menu for testing application. One shotted by gemini (since this has nothing to
     * do with the learning objectives of the course I asked AI to help generate this :-))
     * 
     * */ 
    loop {
        println!("\nSelect the RDT Version for the Receiver:");
        println!("1: Reliable data transfer with positive and negative ACKs");
        println!("2: Reliable data transfer with only positive ACKs");
        println!("3: Reliable data transfer with only negative ACKs");
        println!("4: Fully reliable data transfer with both ACKs and NACKs");
        println!("5: Gbn sender");
        println!("6: Gbn receiver");
        print!("Enter 1, 2, 3, 4, 5 or 6: ");
        
        // Flush stdout to ensure the print! macro displays before taking input
        io::stdout().flush().unwrap();

        let mut input = String::new();
        
        // Read the user's input
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        // Trim whitespace/newlines and match the string slice
        match input.trim() {
            "1" => return RdtVersion::AckNack,
            "2" => return RdtVersion::AckOnly,
            "3" => return RdtVersion::NackOnly,
            "4" => return RdtVersion::FullRdt,
            "5" => return RdtVersion::GbnSender,
            "6" => return RdtVersion::GbnReceiver,
            _ => println!("Invalid input. Please type 1, 2, 3, 4, 5 or 6."),
        }
    }
}
fn handle_only_nack(success: bool) -> Option<[u8; 4]> {
    /* 
     * Handles message received status, always NAK 
     * */
    if success {
        return None;
    }
    let buff = b"NAK";
    let mut nack_packet = [0u8; 4];
    nack_packet[0..3].copy_from_slice(buff);
    nack_packet[0] = 0x12;
    Some(nack_packet)
}

fn handle_only_ack(success: bool, message: &[u8], latest_sequence: u8) -> ([u8; 5], u8) {
    /* 
     * Handles message received status, always ACK. 
     * */
    if success {
        // Extract sequence number from packet
        let sequence_number = message[0];
        let buff: &[u8] = b"ACK";
        let mut ack_packet = [0u8; 5];
        ack_packet[1..4].copy_from_slice(buff);
        
        // Put sequence number as the first byte
        ack_packet[0] = sequence_number;

        // CRC8 changes based on the sequence number
        ack_packet[4] = if sequence_number == 0x00 {0x7f} else {0x69};
        return (ack_packet, sequence_number);
    } 
    
    // Packet was not received properly send ACK of the previous packet 
    let buff: &[u8] = b"ACK";
    let mut ack_packet = [0u8; 5];
    ack_packet[1..4].copy_from_slice(buff);
    ack_packet[0] = latest_sequence;
    ack_packet[4] = if latest_sequence == 0x00 {0x7f} else {0x69};
    
    // Also return the latest sequence here
    return (ack_packet, latest_sequence);

}

fn hanlde_ack_or_nack(success: bool) -> [u8; 4] {
    /* 
     * Handles message received status returns either a NACK or ACK packet
     * */        
    let buff: &[u8] = if success {
        b"ACK"
    } 
    else {
        b"NAK"
    };

    // println!("crc8 for {} is: {crc8}", String::from_utf8_lossy(buff));
    let mut ack_packet = [0u8; 4];
    ack_packet[0..3].copy_from_slice(buff);
    ack_packet[3] = if success {
        0x7f
    }
    else {
        0x12
    };

    ack_packet

}

fn check_for_bit_errors(data: &[u8]) -> u8 {
    /*
     * Checks for bit errors. Iam going to assume that the generator for CRC-8 used is the same we
     * saw in the video i,e 100000111
     * */
    const MASKS: [u8; 8] = [
        0x80,
        0x40,
        0x20,
        0x10,
        0x08,
        0x04,
        0x02,
        0x01,
    ];
    let mut register: u8= 0; // Initiaze register
    for &byte in data {
        for i in 0..8 {
            let msb = (register & MASKS[0]) >> 7;
            let nth_bit = (byte & MASKS[i]) >> (7-i);

            // Perform XOR for bits in indexes 0,1,2
            
            
            let mut first = nth_bit & MASKS[7]; // i.e the bit in the last place so no need to
                                                // move. The first bit also gets its value from the
                                                // incoming bit i.e the nth_bit
            first ^= msb;
            
            let mut second = register & MASKS[7]; // Extracted from the first bit but moved to the
                                                  // second since the new first will come first
            second = (second ^ msb) << 1;
           
            let mut third = (register & MASKS[6]) >> 1; // Same here new position will be different
                                                        // than previous
            third = (third ^ msb) << 2;

            // Shift other bits 
            let shifted = (register & 0b0111_1100) << 1;

            // Construct new register
            register = shifted | third | second | first;
        }
    }
    return register;
}

