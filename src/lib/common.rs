pub const FORWARD_TO_KEY: &str = "X-Tunnel-Forward-To";
pub const AUTH_TOKEN_KEY: &str = "X-Tunnel-Token";
pub const DEVICE_NAME_KEY: &str = "device_name";
pub const HEADER_FIXED_LEN: usize = 5;
pub const MAX_DATA_LEN: usize = 1024;
pub const MAX_SNIFF_LEN: usize = 2048;

pub fn get_client_id_from_token(token: &str) -> String {
    token.to_string()
}
