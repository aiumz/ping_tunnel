pub mod tunnel {
    pub mod common;
    pub mod edge;
    pub mod inbound;
    pub mod outbound;
    pub mod packet;
    pub mod session;
    pub mod sniff;
    pub mod supernode;
}

pub mod transport {
    pub mod base;
    pub mod cert;
    pub mod quic;
}

#[cfg(feature = "napi")]
use napi_derive::napi;
#[cfg(feature = "napi")]
use tokio::runtime::Handle;

#[cfg(feature = "napi")]
#[napi]
pub fn connect_to_server(
    server_addr: String,
    token: String,
    forward_to: String,
) -> napi::Result<()> {
    match Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                let _ =
                    crate::tunnel::edge::connect_to_server(server_addr, token, forward_to).await;
            });
        }
        Err(_) => {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                let _ = rt.block_on(async {
                    crate::tunnel::edge::connect_to_server(server_addr, token, forward_to).await
                });
            });
        }
    }
    Ok(())
}

#[cfg(feature = "napi")]
#[napi]
pub struct EdgeClient {
    server_addr: String,
    token: String,
    forward_to: String,
}

#[cfg(feature = "napi")]
#[napi]
impl EdgeClient {
    #[napi(constructor)]
    pub fn new(server_addr: String, token: String, forward_to: String) -> Self {
        Self {
            server_addr,
            token,
            forward_to,
        }
    }

    #[napi]
    pub fn connect(&self) -> napi::Result<()> {
        let server_addr = self.server_addr.clone();
        let token = self.token.clone();
        let forward_to = self.forward_to.clone();
        match Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    let _ = crate::tunnel::edge::connect_to_server(server_addr, token, forward_to)
                        .await;
                });
            }
            Err(_) => {
                std::thread::spawn(move || {
                    let rt =
                        tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                    let _ = rt.block_on(async {
                        crate::tunnel::edge::connect_to_server(server_addr, token, forward_to).await
                    });
                });
            }
        }
        Ok(())
    }

    #[napi]
    pub fn disconnect(&self) -> napi::Result<()> {
        Ok(())
    }

    #[napi]
    pub fn invoke(&self, _command: String, _data: String) -> napi::Result<String> {
        Ok("".to_string())
    }
}
