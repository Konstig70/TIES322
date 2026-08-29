mod virtual_socket;

use virtual_socket::VirtualSocket;
use std::io::{self, Write};

// Stores all rdt versions
pub enum RdtVersion {
    AckNack,
    AckOnly,   
    NackOnly,  
}

fn main() -> std::io::Result<()> {
    {
        let mut socket = VirtualSocket::new("0.0.0.0:8080")?;
        let rdt_version = get_rdt_version_from_user();
        let mut latest_sequence = 0x01; //Assume latest sequence number is 1 
        loop {
            println!("Listening...");
            let mut buffer = [0; 256];
            let (am, addr) = socket.recv_from(&mut buffer).expect("Data not received");
        
            let filled_buf = &mut buffer[..am-1];
            let msg = String::from_utf8_lossy(filled_buf);
            println!("Received {} bytes from {}: {:?}", am, addr, msg);
            //println!("Last bit: 0x{:02x}", buffer[am-1]);
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
            }
        }
    }
}

pub fn get_rdt_version_from_user() -> RdtVersion {
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
        print!("Enter 1, 2, or 3: ");
        
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
            _ => println!("Invalid input. Please type 1, 2, or 3."),
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

