import quicserve

# Client example
client = quicserve.PyClient("127.0.0.1:4433", {
    "ca_path": "path/to/ca.pem",
    "format": "json",
    "timeout_ms": 5000
})

client.connect()
result = client.call("myservice.mymethod", {"param1": "value1"})
client.close()

# Server example
class MyService:
    def call(self, method, payload):
        # Process the request
        return b'{"result": "success"}'
    
    def methods(self):
        return ["method1", "method2"]

server = quicserve.PyServer("127.0.0.1:4433", {
    "cert_path": "path/to/cert.pem",
    "key_path": "path/to/key.pem",
    "format": "json"
})

server.register_service("myservice", MyService())
server.serve_in_background()  # Non-blocking
# or
# server.serve()  # Blocking