//! Additional tests for the S3-compatible API
use reqwest::Client;

#[tokio::test]
async fn test_s3_404() {
    let client = Client::new();
    let url = "http://localhost:5000/s3/testbucket/notfound.txt";
    let resp = client.get(url).send().await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_s3_overwrite() {
    let client = Client::new();
    let url = "http://localhost:5000/s3/testbucket/overwrite.txt";
    let data1 = b"first";
    let data2 = b"second";
    // PUT first
    let put1 = client.put(url).body(data1.as_ref()).send().await.unwrap();
    assert!(put1.status().is_success());
    // PUT overwrite
    let put2 = client.put(url).body(data2.as_ref()).send().await.unwrap();
    assert!(put2.status().is_success());
    // GET
    let get = client.get(url).send().await.unwrap();
    let body = get.bytes().await.unwrap();
    assert_eq!(body.as_ref(), data2);
}

#[tokio::test]
async fn test_s3_multiple_objects() {
    let client = Client::new();
    let url1 = "http://localhost:5000/s3/testbucket/obj1.txt";
    let url2 = "http://localhost:5000/s3/testbucket/obj2.txt";
    let data1 = b"foo";
    let data2 = b"bar";
    // PUT obj1
    let put1 = client.put(url1).body(data1.as_ref()).send().await.unwrap();
    assert!(put1.status().is_success());
    // PUT obj2
    let put2 = client.put(url2).body(data2.as_ref()).send().await.unwrap();
    assert!(put2.status().is_success());
    // GET obj1
    let get1 = client.get(url1).send().await.unwrap();
    let body1 = get1.bytes().await.unwrap();
    assert_eq!(body1.as_ref(), data1);
    // GET obj2
    let get2 = client.get(url2).send().await.unwrap();
    let body2 = get2.bytes().await.unwrap();
    assert_eq!(body2.as_ref(), data2);
}
