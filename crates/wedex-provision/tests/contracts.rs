use wedex_provision::{endpoint, Request, Worker};
fn request() -> Request {
    Request {
        host: "203.0.113.8".into(),
        public_host: "cloud.example.org".into(),
        ssh_port: 22,
        port: 8765,
        user: "root".into(),
        password: "fake".to_owned().into(),
        key_path: String::new(),
        key_passphrase: String::new().into(),
        sudo_password: String::new().into(),
        fingerprint: None,
        device_name: "Test".into(),
        connect_only: false,
        previous: None,
    }
}
#[test]
fn shell_inputs_and_conflicting_ports_are_rejected_before_ssh() {
    for host in [
        "x; reboot",
        "-oProxyCommand=x",
        "https://example.org",
        "a/b",
        "a\nwhoami",
        "0.0.0.0",
        "",
    ] {
        let mut r = request();
        r.host = host.into();
        assert!(r.validate().is_err(), "{host}");
    }
    let mut r = request();
    r.user = "root;whoami".into();
    assert!(r.validate().is_err());
    let mut r = request();
    r.port = 22;
    assert!(r.validate().is_err());
    let mut r = request();
    r.sudo_password = "abc\ncommand".to_owned().into();
    assert!(r.validate().is_err());
}
#[test]
fn ipv6_is_bracketed_and_missing_server_bundle_never_starts_a_worker() {
    assert_eq!(endpoint("2001:db8::12", 443), "https://[2001:db8::12]:443/");
    let r = request();
    assert!(r.validate().is_ok());
    assert!(Worker::start(r, &[]).is_err());
}

#[test]
fn cancellation_does_not_wait_for_a_stalled_ssh_handshake() {
    use std::{
        net::TcpListener,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut r = request();
    r.host = "127.0.0.1".into();
    r.ssh_port = listener.local_addr().unwrap().port();
    r.connect_only = true;
    let mut worker = Worker::start(r, &[]).unwrap();
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(100));
    worker.cancel();
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(wedex_provision::Event::Failed(_)) = worker.events.try_recv() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("Cancellation stalled with SSH");
}
