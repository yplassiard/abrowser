//! Firefox Remote Debugging Protocol (RDP) client
//!
//! RDP uses length-prefixed JSON messages over TCP.
//! Format: `{length}:{json}` where length is the byte count of the JSON part.

use serde_json::{json, Value};
use std::io::{self, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// Firefox RDP client
pub struct RdpClient {
    stream: Arc<Mutex<TcpStream>>,
    reader: Arc<Mutex<BufReader<TcpStream>>>,
    event_tx: mpsc::UnboundedSender<RdpEvent>,
    event_rx: Mutex<mpsc::UnboundedReceiver<RdpEvent>>,
}

/// An RDP event from Firefox
#[derive(Debug, Clone)]
pub struct RdpEvent {
    pub from: String,
    pub event_type: String,
    pub data: Value,
}

impl RdpClient {
    /// Connect to Firefox RDP server
    pub fn connect(port: u16) -> io::Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let stream = TcpStream::connect(&addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;

        let reader = BufReader::new(stream.try_clone()?);

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            reader: Arc::new(Mutex::new(reader)),
            event_tx,
            event_rx: Mutex::new(event_rx),
        })
    }

    /// Send a message to an actor and wait for response
    pub fn send(&self, to: &str, msg_type: &str, params: Value) -> io::Result<Value> {
        let mut message = params.as_object().cloned().unwrap_or_default();
        message.insert("to".to_string(), json!(to));
        message.insert("type".to_string(), json!(msg_type));

        let json = serde_json::to_string(&message)?;
        let packet = format!("{}:{}", json.len(), json);

        // Send the message
        {
            let mut stream = self.stream.lock().unwrap();
            stream.write_all(packet.as_bytes())?;
            stream.flush()?;
        }

        // Read the response
        self.read_response(to)
    }

    /// Send a message without expecting a specific response (for events)
    pub fn send_no_response(&self, to: &str, msg_type: &str, params: Value) -> io::Result<()> {
        let mut message = params.as_object().cloned().unwrap_or_default();
        message.insert("to".to_string(), json!(to));
        message.insert("type".to_string(), json!(msg_type));

        let json = serde_json::to_string(&message)?;
        let packet = format!("{}:{}", json.len(), json);

        let mut stream = self.stream.lock().unwrap();
        stream.write_all(packet.as_bytes())?;
        stream.flush()?;
        Ok(())
    }

    /// Read a single RDP message
    fn read_message(&self) -> io::Result<Value> {
        let mut reader = self.reader.lock().unwrap();

        // Read until we get the colon (length prefix)
        let mut length_str = String::new();
        loop {
            let mut buf = [0u8; 1];
            let n = {
                use std::io::Read;
                reader.get_mut().read(&mut buf)?
            };
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Connection closed"));
            }
            if buf[0] == b':' {
                break;
            }
            length_str.push(buf[0] as char);
        }

        let length: usize = length_str.parse().map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid length: {}", e))
        })?;

        // Read exactly `length` bytes
        let mut json_buf = vec![0u8; length];
        {
            use std::io::Read;
            reader.get_mut().read_exact(&mut json_buf)?;
        }

        let json_str = String::from_utf8(json_buf).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
        })?;

        serde_json::from_str(&json_str).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid JSON: {}", e))
        })
    }

    /// Read response, storing events along the way
    fn read_response(&self, expected_from: &str) -> io::Result<Value> {
        loop {
            let msg = self.read_message()?;

            // Check if this is the response we're waiting for
            if let Some(from) = msg.get("from").and_then(|v| v.as_str()) {
                if from == expected_from {
                    // Check for error
                    if let Some(error) = msg.get("error") {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("RDP error: {:?}", error),
                        ));
                    }
                    return Ok(msg);
                }

                // This is an event or response from another actor
                if let Some(event_type) = msg.get("type").and_then(|v| v.as_str()) {
                    let _ = self.event_tx.send(RdpEvent {
                        from: from.to_string(),
                        event_type: event_type.to_string(),
                        data: msg.clone(),
                    });
                }
            }
        }
    }

    /// Try to receive an event (non-blocking)
    pub fn try_recv_event(&self) -> Option<RdpEvent> {
        let mut rx = self.event_rx.lock().unwrap();
        rx.try_recv().ok()
    }

    /// Receive an event (blocking)
    pub async fn recv_event(&self) -> Option<RdpEvent> {
        // Note: This is a simplified implementation
        // In production, we'd use proper async I/O
        self.try_recv_event()
    }

    /// Read and process any pending messages (call periodically)
    pub fn poll(&self) -> io::Result<()> {
        // Set non-blocking temporarily
        {
            let stream = self.stream.lock().unwrap();
            stream.set_nonblocking(true)?;
        }

        // Try to read messages
        loop {
            match self.read_message() {
                Ok(msg) => {
                    if let Some(from) = msg.get("from").and_then(|v| v.as_str()) {
                        if let Some(event_type) = msg.get("type").and_then(|v| v.as_str()) {
                            let _ = self.event_tx.send(RdpEvent {
                                from: from.to_string(),
                                event_type: event_type.to_string(),
                                data: msg,
                            });
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    // Restore blocking mode and return error
                    let stream = self.stream.lock().unwrap();
                    let _ = stream.set_nonblocking(false);
                    return Err(e);
                }
            }
        }

        // Restore blocking mode
        {
            let stream = self.stream.lock().unwrap();
            stream.set_nonblocking(false)?;
        }

        Ok(())
    }
}

/// Helper to read the initial root actor greeting
pub fn read_root_actor(client: &RdpClient) -> io::Result<String> {
    let msg = client.read_message()?;

    // The initial message contains the root actor info
    // Example: {"from":"root","applicationType":"browser","testConnectionPrefix":"...","traits":{...}}
    if let Some(from) = msg.get("from").and_then(|v| v.as_str()) {
        if from == "root" {
            return Ok("root".to_string());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Expected root actor greeting",
    ))
}
