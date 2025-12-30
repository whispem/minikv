//! Basic test for the S3-compatible API (PUT then GET)
use reqwest::Client;

#[tokio::test]
async fn test_s3_put_get() {
    let client = Client::new();
    let url = "http://localhost:5000/s3/testbucket/hello.txt";
    let data = b"Hello, S3!";
    // PUT
    let put_resp = client.put(url).body(data.as_ref()).send().await.unwrap();
    assert!(put_resp.status().is_success(), "PUT failed: {:?}", put_resp);
    // GET
    let get_resp = client.get(url).send().await.unwrap();
    assert!(get_resp.status().is_success(), "GET failed: {:?}", get_resp);
    let body = get_resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), data, "GET body mismatch");
}
