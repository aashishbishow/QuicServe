use tonic::{Request, Response, Status};
use hello::greeter_server::{Greeter, GreeterServer};
use hello::{HelloReply, HelloRequest};
use rayon::prelude::*;

pub mod hello {
    tonic::include_proto!("hello");
}

#[derive(Debug, Default)]
pub struct MyGreeter;

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Received request: {:?}", request);

        // Parallel processing using Rayon
        let reply_message = (0..5)
            .into_par_iter()
            .map(|_| format!("Hello {}!", request.get_ref().name))
            .collect::<Vec<_>>()
            .join(" ");

        let reply = HelloReply { message: reply_message };

        Ok(Response::new(reply))
    }
}

pub fn create_service() -> GreeterServer<MyGreeter> {
    GreeterServer::new(MyGreeter::default())
}
