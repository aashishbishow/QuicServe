<?php
// Create a server
$server = QuicServe\create_server("127.0.0.1", 8443, "/path/to/cert.pem", "/path/to/key.pem");

// Register a service with a callback
QuicServe\register_service($server, "calculator", "add", function($method, $payload) {
    // Decode the payload (assuming JSON format)
    $data = json_decode($payload, true);
    $result = $data['a'] + $data['b'];
    
    // Return JSON response
    return json_encode(['result' => $result]);
});

// Register another method in the same service
QuicServe\register_service($server, "calculator", "multiply", function($method, $payload) {
    $data = json_decode($payload, true);
    $result = $data['a'] * $data['b'];
    return json_encode(['result' => $result]);
});

// Start the server
QuicServe\start_server($server);

// List available methods
$methods = QuicServe\list_methods($server, "calculator");
print_r($methods); // Would show ["add", "multiply"]