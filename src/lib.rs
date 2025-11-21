// 导出 lib 模块（用于二进制文件）
pub mod lib {
    pub mod cert;
    pub mod client;
    pub mod common;
    pub mod connections;
    pub mod forward;
    pub mod packet;
    pub mod server;
    pub mod sniff;
}

use napi_derive::napi;
use tokio::runtime::Handle;

#[napi]
pub fn connect_to_server(
    server_addr: String,
    token: String,
    forward_to: String,
) -> napi::Result<()> {
    match Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                lib::client::connect_to_server(server_addr, token, forward_to).await;
            });
        }
        Err(_) => {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                rt.block_on(async {
                    lib::client::connect_to_server(server_addr, token, forward_to).await
                });
            });
        }
    }
    Ok(())
}

#[napi]
pub struct EdgeClient {
    server_addr: String,
    token: String,
    forward_to: String,
}

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
                    lib::client::connect_to_server(server_addr, token, forward_to).await;
                });
            }
            Err(_) => {
                std::thread::spawn(move || {
                    let rt =
                        tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                    rt.block_on(async {
                        lib::client::connect_to_server(server_addr, token, forward_to).await
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
