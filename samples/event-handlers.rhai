fn on_unpause(event) {
    print("game unpaused");
}

fn on_pause(event) {
    let client = http::client();
    client.request(#{
        method: "POST",
        url: "http://localhost:8081",
        headers: [
            "Content-Type: application/json",
            "Accept: application/json"
        ],
        body: "{ \"content\": \"Hello from Rhai\" }",
        output: "text" // empty
    });
    ()
}
