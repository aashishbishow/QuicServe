use std::ffi::{c_char, c_int, CStr, CString};
use std::os::raw::{c_long, c_void};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::collections::HashMap;

use anyhow::Result;
use bytes::Bytes;
use log::{debug, error, info};
use tokio::runtime::Runtime;
use async_trait::async_trait;

use crate::{Client, Error, Request, Response, SerializationFormat, Service};

// Additional PHP API functions we need to use for callback support
extern "C" {
    fn zend_call_function(
        function_name: *const c_char,
        function_name_len: usize,
        retval: *mut ZVal,
        param_count: c_int,
        params: *mut ZVal,
        object: *mut c_void,
    ) -> c_int;
    
    fn zval_copy_ctor(zval: *mut ZVal);
    fn zval_ptr_dtor(zval: *mut ZVal);
    fn zend_hash_find(
        ht: *mut c_void,
        name: *const c_char,
        len: usize,
        dest: *mut *mut c_void,
    ) -> c_int;
    
    fn zend_parse_parameters(
        num_args: c_int,
        format: *const c_char,
        ...
    ) -> c_int;
    
    // PHP callback reference functions
    fn zend_create_closure(
        function: *mut c_void,
        called_scope: *mut c_void,
        fci_cache: *mut c_void,
    ) -> *mut c_void;
    
    fn zend_get_closure_invoke_method(
        obj: *mut c_void,
        call_info: *mut *mut c_void,
        fci_cache: *mut *mut c_void,
    ) -> c_int;
    
    fn add_index_zval(
        array: *mut ZVal,
        idx: c_long,
        value: *mut ZVal,
    ) -> c_int;
    
    fn zend_fcall_info_init(
        callable: *mut ZVal,
        check_flags: c_int,
        fci: *mut ZendFcallInfo,
        fci_cache: *mut ZendFcallInfoCache,
        called_scope: *mut *mut c_void,
        object: *mut *mut c_void,
    ) -> c_int;
    
    fn zend_fcall_info_call(
        fci: *mut ZendFcallInfo,
        fci_cache: *mut ZendFcallInfoCache,
        retval_ptr: *mut *mut ZVal,
        param_count: c_int,
        params: *mut *mut ZVal,
    ) -> c_int;
    
    fn zend_fcall_info_args(
        fci: *mut ZendFcallInfo,
        args: *mut ZVal,
    ) -> c_int;
    
    fn zend_fcall_info_args_clear(
        fci: *mut ZendFcallInfo,
        free_mem: c_int,
    );
}

// PHP callback handling structures
#[repr(C)]
struct ZendFcallInfo {
    size: usize,
    function_name: *mut ZVal,
    retval: *mut ZVal,
    params: *mut *mut ZVal,
    object: *mut c_void,
    no_separation: c_int,
    param_count: c_int,
}

#[repr(C)]
struct ZendFcallInfoCache {
    function_handler: *mut c_void,
    called_scope: *mut c_void,
    object: *mut c_void,
}

// PHP callback wrapper
struct PhpCallback {
    callback: *mut ZVal,
    runtime: Arc<Runtime>,
}

impl PhpCallback {
    fn new(callback: *mut ZVal, runtime: Arc<Runtime>) -> Self {
        unsafe {
            // Make a persistent copy of the callback
            let persistent_callback = Box::into_raw(Box::new(ZVal { 
                value: std::ptr::null_mut(),
                type_info: 0 
            }));
            
            // Copy the callback
            std::ptr::copy_nonoverlapping(callback, persistent_callback, 1);
            zval_copy_ctor(persistent_callback);
            
            PhpCallback {
                callback: persistent_callback,
                runtime,
            }
        }
    }
    
    async fn invoke(&self, method: &str, payload: &[u8]) -> Result<Vec<u8>, Error> {
        // Create the actual invocation in a blocking context since PHP callbacks aren't thread-safe
        let method_cstring = CString::new(method).map_err(|_| Error::InvalidMethod)?;
        let method_bytes = method_cstring.as_bytes_with_nul();
        
        let payload_vec = payload.to_vec();
        
        // Execute the PHP callback in a blocking context
        let result = self.runtime.block_on(async move {
            tokio::task::spawn_blocking(move || {
                unsafe {
                    // Prepare callback info
                    let mut fci = ZendFcallInfo {
                        size: std::mem::size_of::<ZendFcallInfo>(),
                        function_name: self.callback,
                        retval: std::ptr::null_mut(),
                        params: std::ptr::null_mut(),
                        object: std::ptr::null_mut(),
                        no_separation: 0,
                        param_count: 0,
                    };
                    
                    let mut fci_cache = ZendFcallInfoCache {
                        function_handler: std::ptr::null_mut(),
                        called_scope: std::ptr::null_mut(),
                        object: std::ptr::null_mut(),
                    };
                    
                    // Initialize function call
                    if zend_fcall_info_init(
                        self.callback,
                        0,
                        &mut fci,
                        &mut fci_cache,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    ) != 0 {
                        return Err(Error::CallbackInvocation("Failed to initialize callback".into()));
                    }
                    
                    // Prepare parameters: method name and payload
                    let mut params = Box::new([
                        Box::into_raw(Box::new(ZVal { value: std::ptr::null_mut(), type_info: 0 })),
                        Box::into_raw(Box::new(ZVal { value: std::ptr::null_mut(), type_info: 0 })),
                    ]);
                    
                    // Set method parameter
                    ZVAL_STRING(params[0], method_cstring.as_ptr(), 1);
                    
                    // Set payload parameter
                    let payload_ptr = payload_vec.as_ptr();
                    let payload_len = payload_vec.len();
                    ZVAL_STRING(
                        params[1], 
                        std::mem::transmute::<*const u8, *const c_char>(payload_ptr),
                        payload_len as c_int,
                    );
                    
                    fci.params = params.as_mut_ptr();
                    fci.param_count = 2;
                    
                    // Create return value
                    let mut retval = Box::new(ZVal { value: std::ptr::null_mut(), type_info: 0 });
                    let retval_ptr = Box::into_raw(retval);
                    
                    // Call the function
                    let call_result = zend_fcall_info_call(
                        &mut fci,
                        &mut fci_cache,
                        &mut retval_ptr,
                        2,
                        fci.params,
                    );
                    
                    // Clean up parameters
                    zval_ptr_dtor(params[0]);
                    zval_ptr_dtor(params[1]);
                    
                    if call_result != 0 {
                        return Err(Error::CallbackInvocation("Failed to call PHP callback".into()));
                    }
                    
                    // Extract result
                    let result = if !retval_ptr.is_null() {
                        let raw_str = Z_STRVAL_P(retval_ptr);
                        let len = Z_STRLEN_P(retval_ptr);
                        
                        if raw_str.is_null() {
                            Err(Error::CallbackInvocation("NULL return value from PHP callback".into()))
                        } else {
                            let bytes = std::slice::from_raw_parts(
                                raw_str as *const u8, 
                                len
                            ).to_vec();
                            
                            // Clean up return value
                            zval_ptr_dtor(retval_ptr);
                            
                            Ok(bytes)
                        }
                    } else {
                        Err(Error::CallbackInvocation("NULL return value from PHP callback".into()))
                    };
                    
                    result
                }
            }).await.unwrap_or_else(|e| Err(Error::CallbackInvocation(format!("Task join error: {}", e))))
        }).await;
        
        result
    }
}

impl Drop for PhpCallback {
    fn drop(&mut self) {
        unsafe {
            if !self.callback.is_null() {
                zval_ptr_dtor(self.callback);
            }
        }
    }
}

// PHP service implementation that delegates to PHP callbacks
struct PhpServiceImpl {
    methods: HashMap<String, PhpCallback>,
    runtime: Arc<Runtime>,
}

impl PhpServiceImpl {
    fn new(runtime: Arc<Runtime>) -> Self {
        PhpServiceImpl {
            methods: HashMap::new(),
            runtime,
        }
    }
    
    fn register_method(&mut self, method: String, callback: PhpCallback) {
        self.methods.insert(method, callback);
    }
}

#[async_trait]
impl Service for PhpServiceImpl {
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, Error> {
        if let Some(callback) = self.methods.get(method) {
            let result = callback.invoke(method, &payload).await?;
            Ok(Bytes::from(result))
        } else {
            Err(Error::MethodNotFound(method.to_string()))
        }
    }
    
    fn methods(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
    }
}

// Service registry to keep track of PHP services
struct PhpServiceRegistry {
    services: HashMap<String, Arc<dyn Service>>,
}

impl PhpServiceRegistry {
    fn new() -> Self {
        PhpServiceRegistry {
            services: HashMap::new(),
        }
    }
    
    fn register_service(&mut self, name: String, service: Arc<dyn Service>) {
        self.services.insert(name, service);
    }
    
    fn get_service(&self, name: &str) -> Option<Arc<dyn Service>> {
        self.services.get(name).cloned()
    }
    
    fn list_services(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }
}

// Global service registry
static mut PHP_SERVICE_REGISTRY: Option<Mutex<PhpServiceRegistry>> = None;

// Initialize service registry
fn init_service_registry() {
    unsafe {
        if PHP_SERVICE_REGISTRY.is_none() {
            PHP_SERVICE_REGISTRY = Some(Mutex::new(PhpServiceRegistry::new()));
        }
    }
}

// Updated server structure with registry
struct PhpQuicServeServer {
    server: crate::server::Server,
    runtime: Arc<Runtime>,
    service_impl: Arc<PhpServiceImpl>,
}

/// Enhanced register service function to fully support PHP callbacks
/// 
/// PHP function: QuicServe\register_service(resource $server, string $name, callable $handler): bool
#[no_mangle]
pub extern "C" fn zif_quicserve_register_service_enhanced(
    return_value: *mut ZVal,
    server_resource: *const ZVal,
    service_name: *const ZVal,
    method_name: *const ZVal,
    callback: *const ZVal,
) {
    unsafe {
        let server_ptr = zend_fetch_resource(
            server_resource as *mut c_void,
            b"QuicServe\\Server\0".as_ptr() as *const c_char,
            SERVER_RESOURCE_ID,
        ) as *mut PhpQuicServeServer;
        
        if server_ptr.is_null() {
            php_error(
                E_WARNING,
                b"Invalid QuicServe server resource\0".as_ptr() as *const c_char,
            );
            ZVAL_BOOL(return_value, 0);
            return;
        }
        
        let server_ref = &mut *server_ptr;
        
        // Get service name
        let service_name_str = CStr::from_ptr(Z_STRVAL_P(service_name))
            .to_str()
            .unwrap_or_default()
            .to_string();
            
        // Get method name
        let method_name_str = CStr::from_ptr(Z_STRVAL_P(method_name))
            .to_str()
            .unwrap_or_default()
            .to_string();
        
        // Create PHP callback wrapper
        let php_callback = PhpCallback::new(callback as *mut ZVal, Arc::clone(&server_ref.runtime));
        
        // Register the method in the service implementation
        server_ref.service_impl.register_method(method_name_str, php_callback);
        
        // Register the service with the server
        let register_result = server_ref.runtime.block_on(async {
            if !server_ref.server.has_service(&service_name_str) {
                // Register new service
                server_ref.server.register_service(
                    &service_name_str, 
                    Arc::clone(&server_ref.service_impl) as Arc<dyn Service>
                ).await
            } else {
                // Service already registered, just return Ok
                Ok(())
            }
        });
        
        match register_result {
            Ok(_) => {
                ZVAL_BOOL(return_value, 1);
            }
            Err(e) => {
                let error_msg = CString::new(format!("Failed to register service: {}", e))
                    .unwrap_or_else(|_| CString::new("Unknown error").unwrap());
                php_error(E_WARNING, error_msg.as_ptr());
                ZVAL_BOOL(return_value, 0);
            }
        }
    }
}

/// Enhanced server creation with PHP callback support
/// 
/// PHP function: QuicServe\create_server(string $host, int $port, string $cert_path, string $key_path): resource
#[no_mangle]
pub extern "C" fn zif_quicserve_create_server_enhanced(
    return_value: *mut ZVal,
    host: *const ZVal,
    port: *const ZVal,
    cert_path: *const ZVal,
    key_path: *const ZVal,
) {
    unsafe {
        let runtime = match TOKIO_RUNTIME.as_ref() {
            Some(rt) => Arc::clone(rt),
            None => {
                php_error(
                    E_ERROR,
                    b"QuicServe runtime not initialized\0".as_ptr() as *const c_char,
                );
                ZVAL_NULL(return_value);
                return;
            }
        };

        let host_str = CStr::from_ptr(Z_STRVAL_P(host))
            .to_str()
            .unwrap_or_default();
        let port_num = Z_LVAL_P(port) as u16;
        
        let cert_path_str = CStr::from_ptr(Z_STRVAL_P(cert_path))
            .to_str()
            .unwrap_or_default();
        
        let key_path_str = CStr::from_ptr(Z_STRVAL_P(key_path))
            .to_str()
            .unwrap_or_default();

        // Create the PHP service implementation
        let service_impl = Arc::new(PhpServiceImpl::new(Arc::clone(&runtime)));

        let server_result = runtime.block_on(async {
            crate::server::Server::new(&format!("{}:{}:{}:{}", host_str, port_num, cert_path_str, key_path_str)).await
        });

        match server_result {
            Ok(server) => {
                // Create PHP resource with enhanced service support
                let php_server = Box::new(PhpQuicServeServer {
                    server,
                    runtime: Arc::clone(&runtime),
                    service_impl: Arc::clone(&service_impl),
                });
                let php_server_ptr = Box::into_raw(php_server);
                
                let resource = zend_register_resource(
                    php_server_ptr as *mut c_void,
                    SERVER_RESOURCE_ID,
                );
                
                // Return the resource
                ZVAL_LONG(return_value, resource as c_long);
            }
            Err(e) => {
                let error_msg = CString::new(format!("Failed to create server: {}", e))
                    .unwrap_or_else(|_| CString::new("Unknown error").unwrap());
                php_error(E_WARNING, error_msg.as_ptr());
                ZVAL_NULL(return_value);
            }
        }
    }
}

/// List registered service methods
/// 
/// PHP function: QuicServe\list_methods(resource $server, string $service_name): array
#[no_mangle]
pub extern "C" fn zif_quicserve_list_methods(
    return_value: *mut ZVal,
    server_resource: *const ZVal,
    service_name: *const ZVal,
) {
    unsafe {
        let server_ptr = zend_fetch_resource(
            server_resource as *mut c_void,
            b"QuicServe\\Server\0".as_ptr() as *const c_char,
            SERVER_RESOURCE_ID,
        ) as *mut PhpQuicServeServer;
        
        if server_ptr.is_null() {
            php_error(
                E_WARNING,
                b"Invalid QuicServe server resource\0".as_ptr() as *const c_char,
            );
            ZVAL_NULL(return_value);
            return;
        }
        
        let server_ref = &*server_ptr;
        
        // Get method list from the service
        let methods = server_ref.service_impl.methods();
        
        // Create PHP array to return
        let array = emalloc(std::mem::size_of::<ZVal>()) as *mut ZVal;
        ZVAL_NULL(array); // Initialize as empty array
        
        // Add methods to array
        for (i, method) in methods.iter().enumerate() {
            let method_zval = emalloc(std::mem::size_of::<ZVal>()) as *mut ZVal;
            let method_cstr = CString::new(method.as_str()).unwrap_or_default();
            
            ZVAL_STRING(method_zval, method_cstr.as_ptr(), 1);
            add_index_zval(array, i as c_long, method_zval);
        }
        
        // Return the array
        std::ptr::copy_nonoverlapping(array, return_value, 1);
    }
}

// Enhanced PHP module initialization
#[no_mangle]
pub extern "C" fn php_quicserve_init_enhanced() -> c_int {
    unsafe {
        // Initialize the original resources
        let result = php_quicserve_init();
        if result != 0 {
            return result;
        }
        
        // Initialize service registry
        init_service_registry();
        
        0 // Success
    }
}

// Add these errors to the crate::error::Error enum in src/error.rs:
/*
/// Additional errors for PHP callback support
#[derive(Error, Debug)]
pub enum Error {
    // ... existing errors ...
    
    /// Error invoking PHP callback
    #[error("Callback invocation error: {0}")]
    CallbackInvocation(String),
}
*/