#!/usr/bin/env python3
from coincurve import PrivateKey

SECRET = "2566de5a6ce180c0ac85bf93d0d4ca2e896cca0eab8466c8f69270d7f9b98df7"
pk = PrivateKey(bytes.fromhex(SECRET))
print(pk.public_key.format(compressed=True).hex())
