# Test fixtures: throwaway keys

Everything in this folder is a **throwaway test key or certificate**, made with OpenSSL only for the unit tests in `src/cfdi.rs` and `src/pagos.rs`. None of it was issued by the SAT, and none of it belongs to a real taxpayer. Never use these files to sign a real CFDI.

| File | What it is |
|---|---|
| `test-csd.cer` | A self-signed DER certificate, subject `KHATAOS TEST CSD NOT A REAL CERTIFICATE`, carrying the SAT's public test RFC `EKU9003173C9`. |
| `test-csd.key` | Its private key, encrypted DER (PKCS#8), in the form the SAT ships CSD keys. The password is in the tests. |
| `test-csd.key.pem` | The same key as encrypted PEM. |
| `other.key` | A second, unrelated key, used to check that a key not matching the certificate is refused. |
| `openssl-sello.b64` | A sello made with `openssl dgst -sha256 -sign` over a test cadena original, to check that KhataOS produces the same bytes. |

The other test keys in the workspace are throwaway too: the RSA key pair in `crates/khata-in/src/irp.rs` (`testing` module, for the mock IRP) and the test certificate in `crates/khata-sa/tests/data/`.
