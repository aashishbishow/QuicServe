#!/bin/bash
# scripts/generate_certificates.sh
# Script to generate self-signed certificates for QuicServe
set -e
# Create directories
mkdir -p certs
# Generate CA private key and certificate
openssl genrsa -out certs/ca.key 4096
openssl req -new -x509 -days 3650 -key certs/ca.key -out certs/ca.crt \
    -subj "/C=US/ST=CA/L=San Francisco/O=QuicServe/OU=Dev/CN=QuicServe-CA"
# Generate server private key
openssl genrsa -out certs/server.key 4096
# Generate server certificate signing request
openssl req -new -key certs/server.key -out certs/server.csr \
    -subj "/C=US/ST=CA/L=San Francisco/O=QuicServe/OU=Server/CN=localhost"
# Create server certificate extensions file
cat > certs/server.ext << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names
[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1
IP.2 = ::1
EOF
# Sign server certificate with CA
openssl x509 -req -in certs/server.csr -CA certs/ca.crt -CAkey certs/ca.key \
    -CAcreateserial -out certs/server.crt -days 1825 -extfile certs/server.ext
# Generate client private key
openssl genrsa -out certs/client.key 4096
# Generate client certificate signing request
openssl req -new -key certs/client.key -out certs/client.csr \
    -subj "/C=US/ST=CA/L=San Francisco/O=QuicServe/OU=Client/CN=quicserve-client"
# Create client certificate extensions file
cat > certs/client.ext << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names
[alt_names]
DNS.1 = quicserve-client
EOF
# Sign client certificate with CA
openssl x509 -req -in certs/client.csr -CA certs/ca.crt -CAkey certs/ca.key \
    -CAcreateserial -out certs/client.crt -days 1825 -extfile certs/client.ext

# Combine certificates and keys for easier use
cat certs/server.key certs/server.crt > certs/server.pem
cat certs/client.key certs/client.crt > certs/client.pem

# Display success message
echo "Certificates successfully generated in the 'certs' directory"
echo "CA certificate:     certs/ca.crt"
echo "Server certificate: certs/server.crt (with key: certs/server.key, combined: certs/server.pem)"
echo "Client certificate: certs/client.crt (with key: certs/client.key, combined: certs/client.pem)"

# Cleanup temporary files
rm -f certs/*.csr certs/*.ext certs/*.srl

# Make all files read-only for security
chmod 400 certs/*.key certs/*.pem
chmod 444 certs/*.crt

echo "Certificate generation completed."