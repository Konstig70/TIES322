use std::collections::HashMap;
use std::net::{SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use selective_repeat::virtual_socket::VirtualSocket;
use std::io;
 use std::io::Write;

const RECEIVER_ADDR: &str = "127.0.0.1:8080";
const OWN_ADDR: &str = "127.0.0.1:7878";
const WINDOW_SIZE: u8 = 5;

// Enum that describes the packets state 
pub enum PacketStatus {
    InFlight(Instant), 
    Acked,
}

// Struct for whole sender's state 
pub struct SenderState {
    pub oldest_unacked_seq: u8,
    pub next_seq_num: u8,
    pub window_size: u8,
    pub window_buffer: HashMap<u8, (Vec<u8>, PacketStatus)>, // seq num as key, payload and status
                                                             // as value
}

pub fn main() -> std::io::Result<()> {
    /* 
     * Main sending logic, similar to GBN but each packet has its own timer.
     * */

    let socket = VirtualSocket::new(OWN_ADDR)?;
    let state = SenderState {
        oldest_unacked_seq: 0,
        next_seq_num: 0,
        window_size: WINDOW_SIZE,
        window_buffer: HashMap::new(),
    };

    let mutex_state = Arc::new(Mutex::new(state));
    
    // Listener logic 
    let cloned_mutex_state = mutex_state.clone();
    let mut listener_socket = socket.try_clone();
    thread::spawn(move || {
        println!("Starting listen.");

        listener_socket.set_timer(Duration::from_millis(10));
        loop {
            let mut buffer = [0;256];
            match listener_socket.recv_from(&mut buffer) {
                Ok((am,_addr)) => {
                    let filled_buf = &buffer[..am-1];
                    let msg = String::from_utf8_lossy(filled_buf);
                    println!("Received {} bytes: {:?}", am, msg);
            
                    // Checksum 
                    let checksum = check_for_bit_errors(&buffer[..am]);
                    let success = checksum == 0;
                    if success {
                        let mut curr_state = cloned_mutex_state.lock().expect("Thread poisoned!");

                        let seq_num = filled_buf[0];

                        // using unwrap here since I dont really care about safety and 
                        // the checksum should stop any impossible seq nums from being sent. 
                        if let Some(packet_state) = curr_state.window_buffer.get_mut(&seq_num) {
                            packet_state.1 = PacketStatus::Acked;
                        } else {
                            // If it's not in the map, it's an old or duplicate ACK. We just ignore it.
                            println!("Ignoring ACK for sequence {}, already processed.", seq_num);
                        }
                        loop {
                            if let Some((_, status)) = curr_state.window_buffer.get(&curr_state.oldest_unacked_seq) {
                                       
                                match status {
                                    PacketStatus::Acked => {
                                        println!("Packet ACKED, remowing from buffer...");
                                        // Remove packet 
                                        let index = curr_state.oldest_unacked_seq;
                                        curr_state.window_buffer.remove(&index);
                                        // Increment
                                        curr_state.oldest_unacked_seq += 1;
                                        break;
                                }

                                    PacketStatus::InFlight(_) => {
                                        println!("Packet {} still waiting for ACK, stopping loop", curr_state.oldest_unacked_seq);
                                        break;
                                    }
                                }
                            } else {
                                println!("No packet with seq {}!", curr_state.oldest_unacked_seq); 
                                break;
                            }
                        }

                    } else {
                        println!("Bit error in message!")
                    }
                          
                }

                Err(_) => {
                    // Logic for resending
                    let mut curr_state = cloned_mutex_state.lock().expect("Thread poisoned!");


                    let timeout_limit = Duration::from_secs(5);
                    let receiver_addr: SocketAddr = RECEIVER_ADDR.parse().expect("Failed to parse!");

                    // Iterate through every packet
                    for (_seq_num, (payload, state)) in curr_state.window_buffer.iter_mut() {
                        if let PacketStatus::InFlight(sent_time) = state {
                            if sent_time.elapsed() > timeout_limit {
                                println!("Packet {} timedout, resending!", payload[0]);


                                match listener_socket.send_msg(&payload, receiver_addr) {
                                    Ok(am) => {
                                        println!("Wrote {am} bytes");
                                        *state = PacketStatus::InFlight(Instant::now());
                                    },
                                    Err(e) => eprintln!("Error: {e}")
                                }
                            }
                        }
                    }
                }
            }

        }
    });

    loop {
        print!("Input message>");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        input = String::from(input.trim_end());
        // Get bytes
        let input_bytes = input.as_bytes();

        // Create buffer variable here since it needs to live longer
        let mut buffer = Vec::with_capacity(1 + input_bytes.len() + 1);

        let (packet, send_seq) = {
            let mut curr_state = mutex_state.lock().expect("Thread poisoned!");

            if curr_state.next_seq_num < curr_state.oldest_unacked_seq + curr_state.window_size {
                let seq = curr_state.next_seq_num;

                buffer.push(seq);
                buffer.extend_from_slice(input_bytes);
                buffer.push(0x00);

                let crc8 = check_for_bit_errors(&buffer);
                *buffer.last_mut().unwrap() = crc8;

                curr_state.next_seq_num += 1;
                curr_state.window_buffer.insert(seq, (buffer.clone(), PacketStatus::InFlight(Instant::now())));

                (buffer, seq)

            } else {
                println!("Window full, wait please!");
                continue;
            }
        }; // Mutex drops here automagically

        let receiver_addr: SocketAddr = RECEIVER_ADDR.parse().expect("Failed to parse!");
        match socket.send_msg(&packet, receiver_addr) {
            Ok(am) => println!("Sent {} bytes", am),
            Err(e) => eprintln!("Error {e} while sending seq: {}", send_seq)
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
