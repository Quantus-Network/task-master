pub fn parse_x_status_url(url: &str) -> Option<(String, String)> {
    let url_path = url.split('?').next()?.trim();

    // We check if it contains distinct X/Twitter domains
    let valid_domains = ["x.com/", "twitter.com/"];
    if !valid_domains.iter().any(|d| url_path.contains(d)) {
        // Returns None if input is "google.com/..." or "random text"
        return None;
    }

    // Find anchor "/status/"
    let split_keyword = "/status/";
    let index = url_path.rfind(split_keyword)?;

    // Extract Username
    let prefix = &url_path[..index];
    let username = prefix.split('/').last()?;

    if username.is_empty() {
        return None;
    }

    // Extract ID
    let id_part = &url_path[index + split_keyword.len()..];
    let id = id_part.trim_end_matches('/');

    // Validate ID is numeric (Bad URL protection)
    // If the URL is "x.com/user/status/bad_id", this catches it.
    if id.is_empty() || !id.chars().all(char::is_numeric) {
        return None;
    }

    Some((username.to_string(), id.to_string()))
}

pub fn build_x_status_url(username: &str, id: &str) -> String {
    format!("https://x.com/{}/status/{}", username, id)
}
