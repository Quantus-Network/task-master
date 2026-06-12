pub fn build_x_status_url(username: &str, id: &str) -> String {
    format!("https://x.com/{}/status/{}", username, id)
}
