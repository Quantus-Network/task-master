pub fn parse_x_status_url(url: &str) -> Option<String> {
    let url = url.split('?').next()?;
    let mut parts = url.split('/').skip(3);

    parts.next()?;

    if parts.next()? != "status" {
        return None;
    }

    let id = parts.next()?.to_string();

    Some(id)
}
