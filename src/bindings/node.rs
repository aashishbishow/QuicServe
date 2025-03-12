use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use napi::{CallContext, Env, Error, JsFunction, JsNumber, JsObject, JsString, JsUndefined, Property, Result, Status};
use napi_derive::{js_function, module_exports};
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, RwLock};

use crate::{Client, Config, Server, SerializationFormat, Service};

/// JavaScript runtime
struct JsRuntime {
    /// Tokio runtime for async operations
    runtime: Runtime,
}

impl JsRuntime {
    /// Creates a new JavaScript runtime
    fn new() -> Result<Self> {
        let runtime = Runtime::new()
            .map_err(|e| Error::new(Status::GenericFailure, format!("Failed to create runtime: {}", e)))?;
        
        Ok(Self { runtime })
    }
    
    /// Executes a future in the runtime
    fn block_on<F, T>(&self, future: F) -> T
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime.block_on(future)
    }
}

/// JavaScript representation of a client
struct JsClient {
    /// QuicServe client
    client: Client,
    /// Callback for handling events
    event_callback: Option<napi::JsFunction>,
}

/// JavaScript representation of a server
struct JsServer {
    /// QuicServe server
    server: Arc<Server>,
    /// Registered services
    services: HashMap<String, JsService>,
    /// Callback for handling events
    event_callback: Option<napi::JsFunction>,
}

/// JavaScript service implementation
struct JsService {
    /// Service name
    name: String,
    /// Method handlers
    methods: HashMap<String, napi::JsFunction>,
}

impl Service for JsService {
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, crate::Error> {
        // Get method handler
        let handler = self.methods.get(method)
            .ok_or_else(|| crate::Error::MethodNotFound(format!("Method not found: {}", method)))?;
        
        // TODO: Call JavaScript method handler and convert result
        unimplemented!()
    }
    
    fn methods(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
    }
}

/// Converts a JavaScript configuration object to a QuicServe Config
fn js_config_to_config(env: &Env, js_config: &JsObject) -> Result<Config> {
    // Extract address
    let addr_value = js_config.get_named_property::<JsString>("addr")?;
    let addr_str = addr_value.into_utf8()?.into_owned()?;
    let addr = addr_str.parse()
        .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid address: {}", e)))?;
    
    // Create base config
    let mut config = Config::new(addr);
    
    // Extract optional properties
    if let Ok(cert_path) = js_config.get_named_property::<JsString>("certPath") {
        config.cert_path = Some(cert_path.into_utf8()?.into_owned()?.into());
    }
    
    if let Ok(key_path) = js_config.get_named_property::<JsString>("keyPath") {
        config.key_path = Some(key_path.into_utf8()?.into_owned()?.into());
    }
    
    if let Ok(ca_path) = js_config.get_named_property::<JsString>("caPath") {
        config.ca_path = Some(ca_path.into_utf8()?.into_owned()?.into());
    }
    
    if let Ok(verify_peer) = js_config.get_named_property::<JsObject>("verifyPeer") {
        config.verify_peer = verify_peer.coerce_to_bool()?.get_value()?;
    }
    
    if let Ok(format_str) = js_config.get_named_property::<JsString>("format") {
        let format = match format_str.into_utf8()?.into_owned()?.as_str() {
            "protobuf" => SerializationFormat::Protobuf,
            "json" => SerializationFormat::Json,
            _ => return Err(Error::new(Status::InvalidArg, "Invalid format")),
        };
        config.format = format;
    }
    
    if let Ok(timeout_ms) = js_config.get_named_property::<JsNumber>("timeoutMs") {
        config.timeout_ms = timeout_ms.get_uint32()? as u64;
    }
    
    if let Ok(max_streams) = js_config.get_named_property::<JsNumber>("maxConcurrentStreams") {
        config.max_concurrent_streams = max_streams.get_uint32()? as u64;
    }
    
    if let Ok(keep_alive_ms) = js_config.get_named_property::<JsNumber>("keepAliveMs") {
        config.keep_alive_ms = Some(keep_alive_ms.get_uint32()? as u64);
    }
    
    if let Ok(idle_timeout_ms) = js_config.get_named_property::<JsNumber>("idleTimeoutMs") {
        config.idle_timeout_ms = Some(idle_timeout_ms.get_uint32()? as u64);
    }
    
    if let Ok(server_name) = js_config.get_named_property::<JsString>("serverName") {
        config.server_name = Some(server_name.into_utf8()?.into_owned()?);
    }
    
    Ok(config)
}

/// Creates a new client
#[js_function(1)]
fn create_client(ctx: CallContext) -> Result<JsObject> {
    let js_config = ctx.get::<JsObject>(0)?;
    let config = js_config_to_config(ctx.env, &js_config)?;
    
    // Create runtime
    let runtime = JsRuntime::new()?;
    
    // Create client
    let client = runtime.block_on(async {
        Client::new(config)
            .await
            .map_err(|e| Error::new(Status::GenericFailure, format!("Failed to create client: {}", e)))
    })?;
    
    // Create JavaScript client object
    let mut js_client_obj = ctx.env.create_object()?;
    
    // Store client in external data
    let js_client = JsClient {
        client,
        event_callback: None,
    };
    
    let external = ctx.env.create_external(js_client, None)?;
    js_client_obj.set_named_property("_external", external)?;
    
    // Define methods
    
    // connect method
    let connect_fn = ctx.env.create_function_from_closure("connect", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_client: &mut JsClient = ctx.env.get_value_external(&external)?;
        
        // Create promise
        let deferred = ctx.env.create_deferred()?;
        
        // Execute in runtime
        let runtime = JsRuntime::new()?;
        let client_ref = &js_client.client;
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match client_ref.connect().await {
                    Ok(_) => deferred.resolve(|env| env.get_undefined()),
                    Err(e) => deferred.reject(Error::new(Status::GenericFailure, format!("Failed to connect: {}", e))),
                }
            });
        });
        
        Ok(ctx.env.get_promise(deferred)?)
    })?;
    
    js_client_obj.set_named_property("connect", connect_fn)?;
    
    // call method
    let call_fn = ctx.env.create_function_from_closure("call", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_client: &mut JsClient = ctx.env.get_value_external(&external)?;
        
        // Get method name
        let method = ctx.get::<JsString>(0)?.into_utf8()?.into_owned()?;
        
        // Get request data
        let request_obj = ctx.get::<JsObject>(1)?;
        
        // TODO: Convert request object to bytes
        let request_bytes = Bytes::new(); // Placeholder
        
        // Create promise
        let deferred = ctx.env.create_deferred()?;
        
        // Execute in runtime
        let runtime = JsRuntime::new()?;
        let client_ref = &js_client.client;
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                // Call method
                match client_ref.call::<_, Bytes>(&method, &request_bytes).await {
                    Ok(response) => {
                        // TODO: Convert response bytes to JavaScript object
                        deferred.resolve(|env| env.get_undefined())
                    },
                    Err(e) => deferred.reject(Error::new(Status::GenericFailure, format!("RPC call failed: {}", e))),
                }
            });
        });
        
        Ok(ctx.env.get_promise(deferred)?)
    })?;
    
    js_client_obj.set_named_property("call", call_fn)?;
    
    // close method
    let close_fn = ctx.env.create_function_from_closure("close", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_client: &mut JsClient = ctx.env.get_value_external(&external)?;
        
        // Create promise
        let deferred = ctx.env.create_deferred()?;
        
        // Execute in runtime
        let runtime = JsRuntime::new()?;
        let client_ref = &js_client.client;
        
        std::thread::spawn(move || {
            runtime.block_on(async {
                match client_ref.close().await {
                    Ok(_) => deferred.resolve(|env| env.get_undefined()),
                    Err(e) => deferred.reject(Error::new(Status::GenericFailure, format!("Failed to close: {}", e))),
                }
            });
        });
        
        Ok(ctx.env.get_promise(deferred)?)
    })?;
    
    js_client_obj.set_named_property("close", close_fn)?;
    
    // on method for event handling
    let on_fn = ctx.env.create_function_from_closure("on", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_client: &mut JsClient = ctx.env.get_value_external(&external)?;
        
        // Get event name
        let event = ctx.get::<JsString>(0)?.into_utf8()?.into_owned()?;
        
        // Get callback
        let callback = ctx.get::<JsFunction>(1)?;
        
        // Store callback
        js_client.event_callback = Some(callback);
        
        Ok(ctx.env.get_undefined()?)
    })?;
    
    js_client_obj.set_named_property("on", on_fn)?;
    
    // Return client object
    Ok(js_client_obj)
}

/// Creates a new server
#[js_function(1)]
fn create_server(ctx: CallContext) -> Result<JsObject> {
    let js_config = ctx.get::<JsObject>(0)?;
    let config = js_config_to_config(ctx.env, &js_config)?;
    
    // Create runtime
    let runtime = JsRuntime::new()?;
    
    // Create server
    let server = runtime.block_on(async {
        Server::new(config)
            .await
            .map_err(|e| Error::new(Status::GenericFailure, format!("Failed to create server: {}", e)))
    })?;
    
    // Create JavaScript server object
    let mut js_server_obj = ctx.env.create_object()?;
    
    // Store server in external data
    let js_server = JsServer {
        server: Arc::new(server),
        services: HashMap::new(),
        event_callback: None,
    };
    
    let external = ctx.env.create_external(js_server, None)?;
    js_server_obj.set_named_property("_external", external)?;
    
    // Define methods
    
    // registerService method
    let register_service_fn = ctx.env.create_function_from_closure("registerService", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_server: &mut JsServer = ctx.env.get_value_external(&external)?;
        
        // Get service name
        let name = ctx.get::<JsString>(0)?.into_utf8()?.into_owned()?;
        
        // Get service handler object
        let handler_obj = ctx.get::<JsObject>(1)?;
        
        // Get method names
        let methods = handler_obj.get_property_names()?;
        let length = methods.get_array_length()?;
        
        // Create JS service
        let mut js_service = JsService {
            name: name.clone(),
            methods: HashMap::new(),
        };
        
        // Store method handlers
        for i in 0..length {
            let method_name = methods.get_element::<JsString>(i)?;
            let method_name_str = method_name.into_utf8()?.into_owned()?;
            
            let method = handler_obj.get_property::<JsFunction>(&method_name)?;
            js_service.methods.insert(method_name_str, method);
        }
        
        // Register service with server
        let runtime = JsRuntime::new()?;
        let server_ref = js_server.server.clone();
        let service_arc = Arc::new(js_service.clone());
        
        runtime.block_on(async {
            server_ref.register_service(&name, service_arc).await
                .map_err(|e| Error::new(Status::GenericFailure, format!("Failed to register service: {}", e)))
        })?;
        
        // Store service in JS server
        js_server.services.insert(name, js_service);
        
        Ok(ctx.env.get_undefined()?)
    })?;
    
    js_server_obj.set_named_property("registerService", register_service_fn)?;
    
    // serve method
    let serve_fn = ctx.env.create_function_from_closure("serve", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_server: &mut JsServer = ctx.env.get_value_external(&external)?;
        
        // Create promise
        let deferred = ctx.env.create_deferred()?;
        
        // Clone server for thread
        let server_clone = js_server.server.clone();
        
        // Start server in a separate thread
        std::thread::spawn(move || {
            // Create runtime for thread
            let runtime = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    deferred.reject(Error::new(Status::GenericFailure, format!("Failed to create runtime: {}", e)));
                    return;
                }
            };
            
            // Run server
            runtime.block_on(async {
                match server_clone.serve().await {
                    Ok(_) => deferred.resolve(|env| env.get_undefined()),
                    Err(e) => deferred.reject(Error::new(Status::GenericFailure, format!("Server error: {}", e))),
                }
            });
        });
        
        Ok(ctx.env.get_promise(deferred)?)
    })?;
    
    js_server_obj.set_named_property("serve", serve_fn)?;
    
    // on method for event handling
    let on_fn = ctx.env.create_function_from_closure("on", move |ctx| {
        let this = ctx.this_unchecked::<JsObject>()?;
        let external = this.get_named_property::<JsObject>("_external")?;
        let js_server: &mut JsServer = ctx.env.get_value_external(&external)?;
        
        // Get event name
        let event = ctx.get::<JsString>(0)?.into_utf8()?.into_owned()?;
        
        // Get callback
        let callback = ctx.get::<JsFunction>(1)?;
        
        // Store callback
        js_server.event_callback = Some(callback);
        
        Ok(ctx.env.get_undefined()?)
    })?;
    
    js_server_obj.set_named_property("on", on_fn)?;
    
    // Return server object
    Ok(js_server_obj)
}

/// Initialize the module
#[module_exports]
fn init(mut exports: JsObject) -> Result<()> {
    exports.create_named_method("createClient", create_client)?;
    exports.create_named_method("createServer", create_server)?;
    Ok(())
}