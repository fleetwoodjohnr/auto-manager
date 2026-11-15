use super::*;
use crate::secrets::Memory;
use std::sync::Arc;
use wiremock::matchers::{basic_auth, bearer_token, body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn keys() -> Keys {
    Keys { client_id: "test-client".into(), client_secret: "test-secret".into(), production: false }
}

fn urls(base: &str) -> Urls {
    Urls {
        auth: format!("{base}/authorize"),
        token: format!("{base}/token"),
        revoke: format!("{base}/revoke"),
        api: base.to_string(),
        redirect: "http://127.0.0.1:{port}/quickbooks/callback".into(),
    }
}

fn state_of(s: &SignIn) -> String {
    reqwest::Url::parse(&s.url).unwrap().query_pairs().find(|(k, _)| k == "state").unwrap().1.into_owned()
}

/// The browser following Intuit's redirect to this computer.
fn browser_get(s: &SignIn, target: String) -> std::thread::JoinHandle<String> {
    let port = s.listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        // Never wait forever on an answer that isn't coming.
        c.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        c.write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes()).unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        out
    })
}

#[test]
fn sign_in_is_answered_on_this_computer_and_checks_the_state() {
    let s = begin(&keys(), &urls("https://appcenter.example")).unwrap();
    let state = state_of(&s);
    let port = s.listener.local_addr().unwrap().port();
    assert!(state.starts_with(&format!("{port}.")) && state.len() == port.to_string().len() + 1 + 48, "{state}");
    assert!(s.url.contains("scope=com.intuit.quickbooks.accounting") && s.url.contains("client_id=test-client"), "{}", s.url);
    assert_eq!(s.redirect, format!("http://127.0.0.1:{port}/quickbooks/callback"));

    // The browser asks for other things first (a favicon); the listener answers 404 and keeps
    // waiting. Sent one after the other: the listener stops at the answer it wants.
    let s = std::sync::Arc::new(s);
    let waiter = {
        let s = s.clone();
        std::thread::spawn(move || s.wait(Duration::from_secs(20)))
    };
    assert!(browser_get(&s, "/favicon.ico".into()).join().unwrap().starts_with("HTTP/1.1 404"));
    let page = browser_get(&s, format!("/quickbooks/callback?code=C0DE&state={state}&realmId=9130")).join().unwrap();
    let cb = waiter.join().unwrap().unwrap();
    assert_eq!(cb, Callback { code: "C0DE".into(), realm: "9130".into() });
    assert!(page.contains("connected to QuickBooks") && page.contains("default-src 'none'"), "{page}");
}

#[test]
fn a_forged_declined_or_pasted_answer() {
    let s = begin(&keys(), &urls("https://appcenter.example")).unwrap();
    let state = state_of(&s);
    let forged = browser_get(&s, "/quickbooks/callback?code=X&state=1.bad&realmId=9130".into());
    let e = s.wait(Duration::from_secs(5)).unwrap_err();
    assert!(e.to_string().contains("isn't from this sign-in"), "{e}");
    assert!(forged.join().unwrap().contains("Not connected"));

    let s2 = begin(&keys(), &urls("https://appcenter.example")).unwrap();
    s2.paste(&format!("https://fleetwoodjohnr.github.io/auto-manager/quickbooks/?error=access_denied&state={}", state_of(&s2)));
    assert!(s2.wait(Duration::from_secs(5)).unwrap_err().to_string().contains("wasn't allowed"));

    let s3 = begin(&keys(), &urls("https://appcenter.example")).unwrap();
    s3.paste(&format!("https://fleetwoodjohnr.github.io/auto-manager/quickbooks/?code=Z&state={}&realmId=12;DROP", state_of(&s3)));
    assert!(s3.wait(Duration::from_secs(5)).is_err(), "a company id that isn't a number is refused");
    let _ = state;

    let s4 = begin(&keys(), &urls("https://appcenter.example")).unwrap();
    s4.cancel();
    assert!(s4.wait(Duration::from_secs(5)).unwrap_err().to_string().contains("cancelled"));
}

fn token_reply(access: &str, refresh: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "access_token": access, "refresh_token": refresh, "token_type": "bearer", "expires_in": 3600, "x_refresh_token_expires_in": 8726400
    }))
}

async fn mock_company(server: &MockServer, token: &str) {
    Mock::given(method("GET"))
        .and(path("/v3/company/9130/companyinfo/9130"))
        .and(bearer_token(token))
        .and(query_param("minorversion", "75"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "CompanyInfo": { "CompanyName": "Sandbox Garage" } })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn connects_refreshes_rotated_tokens_and_reads_every_page() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(basic_auth("test-client", "test-secret"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=C0DE"))
        .respond_with(token_reply("a1", "r1"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(basic_auth("test-client", "test-secret"))
        .and(body_string_contains("refresh_token=r1"))
        .respond_with(token_reply("a2", "r2"))
        .mount(&server)
        .await;
    mock_company(&server, "a1").await;
    // The first access token has expired by the time the data is read.
    Mock::given(path("/v3/company/9130/query")).and(bearer_token("a1")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
    let items: Vec<Value> = (1..=1001)
        .map(|i| serde_json::json!({ "Id": i.to_string(), "Name": format!("Part {i}"), "FullyQualifiedName": format!("Part {i}"), "Type": "Inventory", "Active": true }))
        .collect();
    for (start, page) in [("1", &items[..1000]), ("1001", &items[1000..])] {
        Mock::given(method("GET"))
            .and(path("/v3/company/9130/query"))
            .and(bearer_token("a2"))
            .and(query_param("query", format!("select * from Item where Active in (true, false) startposition {start} maxresults 1000")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "QueryResponse": { "Item": page } })))
            .mount(&server)
            .await;
    }
    for entity in ["Customer", "Vendor"] {
        Mock::given(method("GET"))
            .and(path("/v3/company/9130/query"))
            .and(bearer_token("a2"))
            .and(query_param("query", format!("select * from {entity} where Active in (true, false) startposition 1 maxresults 1000")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "QueryResponse": {} })))
            .mount(&server)
            .await;
    }

    let store = Arc::new(Memory::default());
    let online = Online::new(keys(), urls(&server.uri()), Box::new(store.clone()));
    let cb = Callback { code: "C0DE".into(), realm: "9130".into() };
    assert_eq!(online.connect(&cb, "http://127.0.0.1:5/quickbooks/callback").await.unwrap(), "Sandbox Garage");
    assert_eq!(store.get("quickbooks:sandbox:9130").unwrap().as_deref(), Some("r1"));

    let data = online.read_all("9130").await.unwrap();
    assert_eq!(data.items.len(), 1001, "both pages");
    assert_eq!(store.get("quickbooks:sandbox:9130").unwrap().as_deref(), Some("r2"), "the rotated refresh token was kept");
    for r in server.received_requests().await.unwrap() {
        if r.url.path().starts_with("/v3/") {
            assert_eq!(r.method.as_str(), "GET", "only reads reach the QuickBooks API: {}", r.url);
        }
    }
}

#[tokio::test]
async fn expired_sign_in_faults_and_disconnect() {
    let server = MockServer::start().await;
    Mock::given(path("/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({ "error": "invalid_grant" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v3/company/42/companyinfo/42"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Fault": { "Error": [{ "Message": "Invalid query", "Detail": "QueryParserError: bad", "code": "4000" }], "type": "ValidationFault" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/revoke"))
        .and(basic_auth("test-client", "test-secret"))
        .and(body_string_contains("\"token\":\"r9\""))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let store = Arc::new(Memory::default());
    let online = Online::new(keys(), urls(&server.uri()), Box::new(store.clone()));
    let e = online.read_all("9130").await.unwrap_err();
    assert!(e.to_string().contains("connect again"), "no stored sign-in: {e}");
    store.set("quickbooks:sandbox:9130", "old").unwrap();
    let e = online.read_all("9130").await.unwrap_err();
    assert!(e.to_string().contains("connect again"), "Intuit refused the refresh: {e}");

    // An Intuit fault comes through in Intuit's words.
    *online.session.lock().await =
        Some(Session { realm: "42".into(), access: "a".into(), until: Instant::now() + Duration::from_secs(60) });
    let e = online.company_name("42").await.unwrap_err();
    assert_eq!(e.to_string(), "QuickBooks: QueryParserError: bad");
    assert!(online.company_name("../x").await.is_err(), "the company id goes into the path, so it must be a number");

    store.set("quickbooks:sandbox:7", "r9").unwrap();
    assert_eq!(online.disconnect("7").await.unwrap(), None);
    assert_eq!(store.get("quickbooks:sandbox:7").unwrap(), None);
}

#[test]
fn maps_intuit_records() {
    let i = item(&serde_json::json!({
        "Id": "11", "Name": "Front pads", "FullyQualifiedName": "Brakes:Front pads", "Sku": "BP-100", "Type": "Inventory",
        "Description": "Ceramic", "UnitPrice": 89.99, "PurchaseCost": 41.5, "QtyOnHand": 37.5, "ReorderPoint": 4,
        "PrefVendorRef": { "value": "56", "name": "Parts Plus" }, "Active": true, "TrackQtyOnHand": true
    }))
    .unwrap();
    assert_eq!(
        (i.external_id.as_str(), i.name.as_str(), i.category.as_str(), i.kind.as_str()),
        ("11", "Front pads", "Brakes", "inventory")
    );
    assert_eq!(
        (i.price.as_deref(), i.cost.as_deref(), i.qty, i.reorder_point),
        (Some("89.99"), Some("41.5"), Some(Qty::from_milli(37_500)), Some(Qty::units(4)))
    );
    assert_eq!(i.vendor, "Parts Plus");
    assert!(item(&serde_json::json!({ "Id": "1", "Name": "Brakes", "Type": "Category" })).is_none());
    assert_eq!(item(&serde_json::json!({ "Id": "2", "Name": "Labor", "Type": "Service" })).unwrap().kind, "service");

    let c = customer(&serde_json::json!({
        "Id": "58", "DisplayName": "Ada Lovelace", "PrimaryPhone": { "FreeFormNumber": "(555) 555-0100" },
        "PrimaryEmailAddr": { "Address": "ada@example.com" },
        "BillAddr": { "Line1": "1 Analytical Way", "City": "Springfield", "CountrySubDivisionCode": "IL", "PostalCode": "62701" }, "Active": false
    }));
    assert_eq!(c.address, "1 Analytical Way, Springfield, IL, 62701");
    assert!(!c.active);
    let v = vendor(
        &serde_json::json!({ "Id": "56", "DisplayName": "Parts Plus", "GivenName": "Sam", "FamilyName": "Diaz", "AcctNum": "A-77", "WebAddr": { "URI": "https://partsplus.example" } }),
    );
    assert_eq!((v.contact.as_str(), v.account_no.as_str(), v.website.as_str()), ("Sam Diaz", "A-77", "https://partsplus.example"));
}
