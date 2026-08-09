use std::net::SocketAddr;
use std::{net::UdpSocket};
use std::io::Result;
use std::thread;
use std::time::Duration;
use rand::RngExt;
use rand::rngs::ThreadRng;

// Rust does not support inheritance the same way that Java etc. support so we need to make a
// Wrapper
pub struct VirtualSocket {
    true_socket: UdpSocket, // The actual socket that we use to read data, etc.
    p_drop: f64, // Probability to drop packet
    p_delay: f64, // Probability of delay 
    p_bit_err: f64,  // Probability of a bit error happening
    rng: ThreadRng 
}

//Method implementations
impl VirtualSocket {
    pub fn new(addr: &str) -> Result<Self> {
        let true_socket = UdpSocket::bind(addr)?;
        Ok(Self {
            true_socket,
            p_drop: 0.5,
            p_delay: 0.8,
            p_bit_err: 0.6,
            rng: rand::rng(),
        })  
    }

    pub fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> { 
        loop {
            // Read from actual socket
            let (am, addr) = self.true_socket.recv_from(buffer)?;
            let mut rnd_num: f64 = self.rng.random();       
            
            // Drop the packet randomly
            if rnd_num < self.p_drop {
                println!("Packet Dropped")
            }

            else {
                
                // Delay
                rnd_num = self.rng.random();
                if rnd_num < self.p_delay {
                    print!("Delaying...");
                    thread::sleep(Duration::from_secs(self.rng.random_range(1..=5))); // Delay of 1
                    println!("Finished")                                             // to 5 secs
                }

                // Bit error 
                rnd_num = self.rng.random();
                if rnd_num < self.p_bit_err {
                    let byte_idx = self.rng.random_range(0..=am);
                    let bit_idx = self.rng.random_range(0..=8);
                    buffer[byte_idx] ^= 1 << bit_idx;
                }
                return Ok((am, addr));
            }
            
        }
    }
}


