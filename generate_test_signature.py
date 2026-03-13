#!/usr/bin/env python3
import os
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ed25519
import binascii

# Generate a valid Ed25519 key pair
private_key = ed25519.Ed25519PrivateKey.generate()
public_key = private_key.public_key()

# Create a sample challenge (32 bytes)
challenge = os.urandom(32)

# Sign the challenge
signature = private_key.sign(challenge)

print(f"Public key (hex): {binascii.hexlify(public_key.public_bytes_raw()).decode()}")
print(f"Challenge (hex): {binascii.hexlify(challenge).decode()}")
print(f"Signature (hex): {binascii.hexlify(signature).decode()}")
print(f"Signature length: {len(signature)} bytes")
