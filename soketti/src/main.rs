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
        }
    }
}

