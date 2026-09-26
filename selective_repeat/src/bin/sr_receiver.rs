use std::collections::HashMap;

use selective_repeat::virtual_socket::VirtualSocket;

const RECEIVER_ADDR: &str = "127.0.0.1:7878";
const OWN_ADDR: &str = "127.0.0.1:8080";
const WINDOW_SIZE: u8 = 5;

pub struct ReceiverState {
    pub rcv_base: u8,
    pub out_of_order_buffer: HashMap<u8, Vec<u8>>, // i.e packets that Have been ACKed
}

pub fn main() -> std::io::Result<()>{
    /* 
     * Main logic for SR sender
     * */
    
    let mut socket = VirtualSocket::new(OWN_ADDR)?;
    let mut state = ReceiverState {
        rcv_base: 0,
        out_of_order_buffer: HashMap::new(),
    }; // No need to use MUTEX since receiver can be single threaded.
    loop {
        let mut buffer = [0; 256];
        let (am, addr) = socket.recv_from(&mut buffer).expect("Data not received");
        
        let filled_buf = &mut buffer[..am-1];
        let msg = String::from_utf8_lossy(filled_buf);
        println!("Received {} bytes from {}: {:?}", am, addr, msg);
        let checksum = check_for_bit_errors(&buffer[..am]);
        let success = checksum == 0;  
        if !success {
            println!("Bit error in message! Checksum was {}", checksum);
            continue; // Skip to next loop 
        }

        let seq_num = buffer[0];
        let payload = buffer[1..am-1].to_vec(); // We exclude crc and seq 

        let forward = seq_num.wrapping_sub(state.rcv_base); // How far ahead are we?  
        let backward = state.rcv_base.wrapping_sub(seq_num); // How behind are we?
                                                
        if forward < WINDOW_SIZE { // we are in current window here 
            // Send ACK 
            let buff: &[u8] = b"ACK";
            let mut ack_packet = [0u8; 5];
            ack_packet[1..4].copy_from_slice(buff);
        
            // Put sequence number as the first byte and CRC8 as the last 
            ack_packet[0] = seq_num;
            let crc8 = check_for_bit_errors(&ack_packet);
            ack_packet[4] = crc8;

            match socket.send_ack(addr, &ack_packet) {
                Ok(_) => println!("Sent ACK!"),
                Err(e) => eprintln!("Error {e}")
                
            }

            // Keep the payload so we can "deliver" it 
            if !state.out_of_order_buffer.contains_key(&seq_num) {
                state.out_of_order_buffer.insert(seq_num, payload);
            }

            // Slide our base and "deliver" our received packets 
            loop {
                if let Some(payload) = state.out_of_order_buffer.remove(&state.rcv_base) {

                    // Here we would deliver but just print out what we received
                    let msg = String::from_utf8_lossy(&payload);
                    println!("Delivered message: {}", msg);

                    state.rcv_base += 1;
                } else {
                    break; // No packet for that seq meaning we are either missing one or they are
                           // out-of-order 
                }
            }
        }
        else if backward <= WINDOW_SIZE && backward > 0 { // Older packet from previous window 
            println!("Received older packet with seq: {}", seq_num);

            // Can just ACK dont need to add to buffer anymore
            let buff: &[u8] = b"ACK";
            let mut ack_packet = [0u8; 5];
            ack_packet[1..4].copy_from_slice(buff);
        
            // Put sequence number as the first byte and CRC8 as the last 
            ack_packet[0] = seq_num;
            let crc8 = check_for_bit_errors(&ack_packet);
            ack_packet[4] = crc8;

            match socket.send_ack(addr, &ack_packet) {
                Ok(_) => println!("Sent ACK!"),
                Err(e) => eprintln!("Error {e}")
                
            }

        } else {
            println!("Out of bounds packet, ignoring...");
        }

    }
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
