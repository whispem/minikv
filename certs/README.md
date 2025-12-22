# To generate self-signed certificates for development:
#
# 1. Generate the private key:
#    openssl genrsa -out key.pem 2048
#
# 2. Generate the self-signed certificate:
#    openssl req -new -x509 -key key.pem -out cert.pem -days 365 -subj "/CN=localhost"
#
# Place key.pem and cert.pem in this folder.
#
# For production, use certificates signed by a trusted authority.
