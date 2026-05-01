#![warn(argument_comment_mismatch)]

fn create_darwin_url(base_url: Option<String>, retry_count: usize) -> String {
    let _ = (base_url, retry_count);
    String::new()
}

fn main() {
    let base_url = Some(String::from("https://api.darwin.local"));
    create_darwin_url(base_url, 3);
    create_darwin_url(/*base_url*/ None, 3);
}
