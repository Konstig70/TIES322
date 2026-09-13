use std::net::SocketAddr;
use std::{net::UdpSocket};
use std::io::Result;
use std::{thread, usize};
use std::time::Duration;
use rand::{RngExt};

// Rust does not support inheritance the same way that Java etc. support so we need to make a
// Wrapper
pub struct VirtualSocket {
    true_socket: UdpSocket, // The actual socket that we use to read data, etc.
    p_drop: f64, // Probability to drop packet
    p_delay: f64, // Probability of delay 
    p_bit_err: f64,  // Probability of a bit error happening
}

//Method implementations
impl VirtualSocket {
    pub fn new(addr: &str) -> Result<Self> {
        /*
         * Constructor
         * */
        let true_socket = UdpSocket::bind(addr)?;
        Ok(Self {
            true_socket,
            p_drop: 0.0,
            p_delay: 0.0,
            p_bit_err: 0.0,
        })  
    }

    pub fn set_timer(&self, dur: Duration) {
        self.true_socket.set_read_timeout(Some(dur)).unwrap(); // This can also panic but we dont
                                                               // care :).
    }

    pub fn try_clone(&self) -> VirtualSocket {
        VirtualSocket {
            p_drop: self.p_drop,
            p_delay: self.p_delay,
            p_bit_err: self.p_bit_err,
            true_socket: self.true_socket.try_clone().unwrap(),
        }
    }

    pub fn send_msg(&self, buf: &Vec<u8>, addr: SocketAddr) -> Result<usize> {
        /* 
         * Sends data to specified address from UDP socket
         * */
        self.true_socket.send_to(buf, addr)
    }

    pub fn send_ack(&mut self, addr: SocketAddr, buff: &[u8]) -> Result<usize> {
        // println!("Sending acknowledgement back to {}", addr);
        
        self.true_socket.send_to(&buff, addr)
    }

    pub fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> { 
        /* 
         * Receives data from socket, is also able to simulate delay, bit error, etc.
         * */
        loop {
            // Read from actual socket
            let (am, addr) = self.true_socket.recv_from(buffer)?;

            let mut rnd_num = rand::rng().random::<f64>();
            
            // Drop the packet randomly
            if rnd_num < self.p_drop {
                println!("Packet Dropped")
            }

            else {
                
                // Delay
                rnd_num = rand::rng().random::<f64>();
                if rnd_num < self.p_delay {
                    print!("Delaying...");
                    thread::sleep(Duration::from_secs(rand::rng().random_range(1..=5))); // Delay of 1
                    println!("Finished")                                             // to 5 secs
                }

                // Bit error 
                rnd_num = rand::rng().random::<f64>();
                if rnd_num < self.p_bit_err {
                    let byte_idx = rand::rng().random_range(1..=5);
                    let bit_idx = rand::rng().random_range(1..=5);
                    buffer[byte_idx] ^= 1 << bit_idx;
                }
                return Ok((am, addr));
            }
            
        }
    }
}


