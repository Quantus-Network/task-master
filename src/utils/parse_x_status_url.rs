pub fn parse_x_status_url(url: &str) -> Option<(String, String)> {
    let url = url.split('?').next()?;
    let mut parts = url.split('/').skip(3);

    let username = parts.next()?.to_string();

    if parts.next()? != "status" {
        return None;
    }

    let id = parts.next()?.to_string();

    Some((username, id))
}
