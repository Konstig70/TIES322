mod virtual_socket;

use virtual_socket::VirtualSocket;

fn main() -> std::io::Result<()> {
    {
        let mut socket = VirtualSocket::new("0.0.0.0:8080")?;
        loop {
            println!("Listening...");
            let mut buffer = [0; 256];
            let (am, addr) = socket.recv_from(&mut buffer).expect("Data not received");
        
            let filled_buf = &mut buffer[..am-1];
            let msg = String::from_utf8_lossy(filled_buf);
            println!("Received {} bytes from {}: {:?}", am, addr, msg);
            // println!("Last bit: 0x{:02x}", buffer[am-1]);
            let checksum = check_for_bit_errors(&buffer[..am]);
            let success = checksum == 0;  
            if !success {
                println!("Bit error in message! Checksum was {}", checksum);
            }
            
            let ack_packet = hanlde_ack_or_nack(success);
            handle_only_ack(success, &buffer);

            match socket.send_ack(addr, &ack_packet) {
                Ok(_) => println!("Sent {} to {addr}", String::from_utf8_lossy(&ack_packet)),
                Err(e) => eprintln!("Error {e}")
            }
        }
    }
}

fn handle_only_ack(success: bool, message: &[u8]) -> ([u8; 4], u8) {
    /* 
     * Handles message received status, always ACK 
     * */

    //E
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

