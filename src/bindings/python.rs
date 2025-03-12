use std::sync::Arc;
use std::{thread, time::Duration};
use std::collections::HashMap;

use anyhow::Result;
use bytes::Bytes;
use futures_util::future::Future;
use pyo3::prelude::*;
use pyo3::exceptions::{PyRuntimeError, PyValueError, PyTimeoutError, PyConnectionError};
use pyo3::types::{PyDict, PyBytes, PyList, PyString};
use pyo3::PyResult;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use crate::{Client, Config, Error, Server, Service, SerializationFormat};

/// Converts a QuicServe error to a Python exception
fn err_to_py(err: Error) -> PyErr {
    match err {
        Error::Timeout => PyTimeoutError::new_err(err.to_string()),
        Error::ConnectionClosed | Error::Quic(_) | Error::WebTransport(_) | Error::Http3(_) => 
            PyConnectionError::new_err(err.to_string()),
        Error::InvalidConfig(_) => PyValueError::new_err(err.to_string()),
        _ => PyRuntimeError::new_err(err.to_string()),
    }
}

/// Python wrapper for QuicServe RPC client
#[pyclass]
struct PyClient {
    client: Arc<Client>,
    runtime: Arc<Runtime>,
    format: SerializationFormat,
}

#[pymethods]
impl PyClient {
    /// Creates a new RPC client
    #[new]
    fn new(py: Python<'_>, addr: &str, options: Option<&PyDict>) -> PyResult<Self> {
        let socket_addr = addr.parse()
            .map_err(|e| PyValueError::new_err(format!("Invalid address: {}", e)))?;
        
        // Create default configuration
        let mut config = Config::new(socket_addr);
        
        // Apply options if provided
        if let Some(opts) = options {
            if let Some(cert_path) = opts.get_item("cert_path") {
                config.cert_path = Some(cert_path.extract::<String>()?.into());
            }
            
            if let Some(key_path) = opts.get_item("key_path") {
                config.key_path = Some(key_path.extract::<String>()?.into());
            }
            
            if let Some(ca_path) = opts.get_item("ca_path") {
                config.ca_path = Some(ca_path.extract::<String>()?.into());
            }
            
            if let Some(verify_peer) = opts.get_item("verify_peer") {
                config.verify_peer = verify_peer.extract::<bool>()?;
            }
            
            if let Some(timeout_ms) = opts.get_item("timeout_ms") {
                config.timeout_ms = timeout_ms.extract::<u64>()?;
            }
            
            if let Some(server_name) = opts.get_item("server_name") {
                config.server_name = Some(server_name.extract::<String>()?);
            }
            
            if let Some(format_str) = opts.get_item("format") {
                let format = match format_str.extract::<&str>()? {
                    "json" => SerializationFormat::Json,
                    "protobuf" => SerializationFormat::Protobuf,
                    _ => return Err(PyValueError::new_err("Invalid format, must be 'json' or 'protobuf'")),
                };
                config.format = format;
            }
        }
        
        // Save format for later
        let format = config.format;
        
        // Create Tokio runtime
        let runtime = Arc::new(Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create Tokio runtime: {}", e)))?);
        
        // Create client
        let client = runtime.block_on(async {
            Client::new(config)
                .await
                .map_err(err_to_py)
        })?;
        
        Ok(Self {
            client: Arc::new(client),
            runtime,
            format,
        })
    }
    
    /// Connects to the RPC server
    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let client = self.client.clone();
        let rt = self.runtime.clone();
        
        py.allow_threads(|| {
            rt.block_on(async {
                client.connect().await.map_err(err_to_py)
            })
        })
    }
    
    /// Calls a remote procedure and returns the result
    fn call(&self, py: Python<'_>, method: &str, args: &PyAny) -> PyResult<PyObject> {
        let client = self.client.clone();
        let rt = self.runtime.clone();
        let format = self.format;
        
        // Convert Python object to bytes
        let payload = if args.is_none() {
            Bytes::new()
        } else if let Ok(bytes) = args.downcast::<PyBytes>() {
            Bytes::copy_from_slice(bytes.as_bytes())
        } else {
            // For non-bytes, try to use serialize method provided by the object
            // or fall back to JSON serialization
            if args.hasattr("serialize")? {
                let serialized = args.call_method0("serialize")?;
                let bytes = serialized.downcast::<PyBytes>()
                    .map_err(|_| PyValueError::new_err("serialize() method must return bytes"))?;
                Bytes::copy_from_slice(bytes.as_bytes())
            } else {
                // Use Python's json module as a fallback
                let json = PyModule::import(py, "json")?;
                let json_str = json.call_method1("dumps", (args,))?;
                let json_bytes = json_str.extract::<String>()?;
                Bytes::from(json_bytes.into_bytes())
            }
        };
        
        // Call remote procedure
        let result_bytes = py.allow_threads(|| {
            rt.block_on(async move {
                client.call_raw(method, payload).await.map_err(err_to_py)
            })
        })?;
        
        // Return bytes directly
        Ok(PyBytes::new(py, &result_bytes).into())
    }
    
    /// Closes the connection to the server
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let client = self.client.clone();
        let rt = self.runtime.clone();
        
        py.allow_threads(|| {
            rt.block_on(async {
                client.close().await.map_err(err_to_py)
            })
        })
    }
}

/// Python wrapper around Rust service implementation
struct PythonServiceBridge {
    py_service: PyObject,
    runtime: Arc<Runtime>,
}

#[async_trait::async_trait]
impl Service for PythonServiceBridge {
    async fn call(&self, method: &str, payload: Bytes) -> Result<Bytes, Error> {
        let method_str = method.to_string();
        let payload_bytes = payload.clone();
        
        let gil = Python::acquire_gil();
        let py = gil.python();
        
        // Call the Python service
        let result = py.allow_threads(|| {
            // Convert the payload to Python bytes
            let py_bytes = PyBytes::new(py, &payload_bytes);
            
            // Call the Python method
            let result = self.py_service.call_method1(py, "call", (method_str, py_bytes));
            
            match result {
                Ok(py_result) => {
                    // Convert result back to bytes
                    match py_result.extract::<&PyBytes>(py) {
                        Ok(py_bytes) => Ok(Bytes::copy_from_slice(py_bytes.as_bytes())),
                        Err(_) => Err(Error::Other("Python service returned non-bytes result".into())),
                    }
                },
                Err(e) => {
                    Err(Error::Other(format!("Python service error: {}", e)))
                }
            }
        });
        
        result
    }
    
    fn methods(&self) -> Vec<String> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        
        match self.py_service.call_method0(py, "methods") {
            Ok(py_methods) => {
                match py_methods.extract::<Vec<String>>(py) {
                    Ok(methods) => methods,
                    Err(_) => vec![],
                }
            },
            Err(_) => vec![],
        }
    }
}

/// Python wrapper for QuicServe RPC server
#[pyclass]
struct PyServer {
    server: Option<Arc<Server>>,
    runtime: Arc<Runtime>,
    config: Config,
    services: HashMap<String, Arc<dyn Service>>,
}

#[pymethods]
impl PyServer {
    /// Creates a new RPC server
    #[new]
    fn new(py: Python<'_>, addr: &str, options: Option<&PyDict>) -> PyResult<Self> {
        let socket_addr = addr.parse()
            .map_err(|e| PyValueError::new_err(format!("Invalid address: {}", e)))?;
        
        // Create default configuration
        let mut config = Config::new(socket_addr);
        
        // Apply options if provided
        if let Some(opts) = options {
            if let Some(cert_path) = opts.get_item("cert_path") {
                config.cert_path = Some(cert_path.extract::<String>()?.into());
            }
            
            if let Some(key_path) = opts.get_item("key_path") {
                config.key_path = Some(key_path.extract::<String>()?.into());
            }
            
            if let Some(ca_path) = opts.get_item("ca_path") {
                config.ca_path = Some(ca_path.extract::<String>()?.into());
            }
            
            if let Some(timeout_ms) = opts.get_item("timeout_ms") {
                config.timeout_ms = timeout_ms.extract::<u64>()?;
            }
            
            if let Some(format_str) = opts.get_item("format") {
                let format = match format_str.extract::<&str>()? {
                    "json" => SerializationFormat::Json,
                    "protobuf" => SerializationFormat::Protobuf,
                    _ => return Err(PyValueError::new_err("Invalid format, must be 'json' or 'protobuf'")),
                };
                config.format = format;
            }
        }
        
        // Create Tokio runtime
        let runtime = Arc::new(Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create Tokio runtime: {}", e)))?);
        
        Ok(Self {
            server: None,
            runtime,
            config,
            services: HashMap::new(),
        })
    }
    
    /// Registers a Python service with the server
    fn register_service(&mut self, py: Python<'_>, name: &str, service: PyObject) -> PyResult<()> {
        // Verify service has required methods
        if !service.hasattr(py, "call")? || !service.hasattr(py, "methods")? {
            return Err(PyValueError::new_err(
                "Service must implement 'call(method, payload)' and 'methods()' methods"
            ));
        }
        
        // Create service bridge
        let bridge = PythonServiceBridge {
            py_service: service,
            runtime: self.runtime.clone(),
        };
        
        // Store service
        self.services.insert(name.to_string(), Arc::new(bridge));
        
        Ok(())
    }
    
    /// Starts the server and blocks until it completes
    fn serve(&mut self, py: Python<'_>) -> PyResult<()> {
        let config = self.config.clone();
        let services = self.services.clone();
        let rt = self.runtime.clone();
        
        py.allow_threads(|| {
            rt.block_on(async {
                // Create server
                let server = Server::new(config).await.map_err(err_to_py)?;
                
                // Register services
                for (name, service) in services {
                    server.register_service(&name, service).await.map_err(err_to_py)?;
                }
                
                // Store server instance
                self.server = Some(Arc::new(server.clone()));
                
                // Start server
                server.serve().await.map_err(err_to_py)
            })
        })
    }
    
    /// Starts the server in a background thread
    fn serve_in_background(&mut self, py: Python<'_>) -> PyResult<()> {
        let config = self.config.clone();
        let services = self.services.clone();
        let rt = self.runtime.clone();
        
        // Create server
        let server = rt.block_on(async {
            Server::new(config).await.map_err(err_to_py)
        })?;
        
        // Register services
        for (name, service) in services {
            rt.block_on(async {
                server.register_service(&name, service).await.map_err(err_to_py)
            })?;
        }
        
        // Store server instance
        let server_arc = Arc::new(server);
        self.server = Some(server_arc.clone());
        
        // Start server in background thread
        let handle = thread::spawn(move || {
            rt.block_on(async {
                if let Err(e) = server_arc.serve().await {
                    eprintln!("Server error: {}", e);
                }
            });
        });
        
        // Detach thread
        handle.join().unwrap_or(());
        
        Ok(())
    }
}

/// Enum with serialization format options
#[pyclass]
#[derive(Clone, Copy)]
enum PySerializationFormat {
    Json = 0,
    Protobuf = 1,
}

/// Module initialization function
#[pymodule]
fn quicserve(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyClient>()?;
    m.add_class::<PyServer>()?;
    m.add_class::<PySerializationFormat>()?;
    
    // Initialize logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,quicserve=debug");
    }
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info")
    );
    
    Ok(())
}

/// Helper function to add a RPC client class to Python module
pub fn register_python_module(py: Python<'_>) -> PyResult<()> {
    let module = PyModule::new(py, "quicserve")?;
    
    module.add_class::<PyClient>()?;
    module.add_class::<PyServer>()?;
    module.add_class::<PySerializationFormat>()?;
    
    PyModule::import(py, "sys")?
        .getattr("modules")?
        .set_item("quicserve", module)?;
    
    Ok(())
}