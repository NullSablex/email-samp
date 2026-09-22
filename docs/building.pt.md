# Compilando do código-fonte

## Requisitos

- Rust estável com os alvos `i686-unknown-linux-gnu` e `i686-pc-windows-msvc`
- `cargo-xwin`, para compilar o `.dll` do Windows a partir do Linux (o script instala)
- **Suporte a C de 32 bits** no alvo Linux: a camada de TLS compila C e assembly de 32 bits, então `gcc -m32` precisa funcionar — `apt install gcc-multilib g++-multilib` no Debian/Ubuntu, `glibc-devel.i686` no Fedora, `lib32-glibc` no Arch
- **LLVM**, só ao compilar o `.dll` do Windows a partir do Linux: empacotar os objetos do TLS para MSVC exige o `llvm-lib`, que o `cargo-xwin` não traz. O `scripts/build-linux.sh` encontra em `/usr/lib/llvm-*/bin`
- Nenhuma biblioteca de e-mail ou TLS do sistema: sem PHPMailer, sem `sendmail`, sem OpenSSL

## Compilando

```bash
./scripts/build-linux.sh              # .so do Linux + .dll do Windows, em dist/
./scripts/build-windows.sh            # a partir do Windows
PROFILE=dev ./scripts/build-linux.sh  # build de desenvolvimento
```

Os scripts leem o nome do plugin no `Cargo.toml`, instalam o alvo do rustup que faltar e produzem `dist/email_samp.so` e `dist/email_samp.dll`.

Para um build simples de um alvo só:

```bash
cargo build --target i686-unknown-linux-gnu
cargo test --target i686-unknown-linux-gnu
cargo clippy --target i686-unknown-linux-gnu --all-targets -- -D warnings
```

## Por que as dependências são essas

Cada escolha existe para manter o build em Rust puro, porque o plugin precisa compilar para Linux 32 bits **e** MSVC 32 bits, onde um toolchain de C é exatamente o que ninguém quer configurar.

### `samp` (rust-samp)

Vem do crates.io e não da tag do git: a árvore publicada é a mesma (a v3.5.0 foi marcada sem bumpar o número, daí o `3.4.0`), compila sem clonar nada e o Dependabot acompanha dependência de registro direito.

Dois recursos ligados:

- `encoding` lê as strings de 8 bits do Pawn na página de código do servidor, então um nick com acento chega inteiro no e-mail em vez de virar `Jo?o`. Veja [`charset`](configuration.md#charset).
- `compression` comprime em gzip os arquivos de log rotacionados, usando `flate2`, que é Rust puro.

### `lettre`

O cliente SMTP e o montador MIME, com **`default-features = false`**. O conjunto padrão traz o `native-tls`, que amarra o OpenSSL e faria o build cruzado i686/MSVC precisar de toolchain C e bibliotecas prontas. Os recursos mantidos são o equivalente em Rust puro:

| Recurso | Para quê |
|---|---|
| `smtp-transport` | o cliente SMTP em si |
| `pool` | reaproveitar conexão, para uma conta manter a sessão em vez de pagar TCP + TLS + AUTH por mensagem |
| `builder` | montagem MIME: multipart, anexos, codificação de cabeçalho (RFC 2047) |
| `hostname` | o nome real da máquina no EHLO; alguns relays recusam `localhost` |
| `rustls-tls` | TLS pelo rustls |

O `rustls-tls` implica `ring` e `webpki-roots`. `ring` em vez do `aws-lc-rs` padrão porque é o que compila para i686, MSVC incluído.

!!! warning "O repositório de certificados é compilado junto"
    `webpki-roots` significa que a verificação de TLS usa o pacote de raízes dentro do binário, **não** o do sistema operacional. Provedores públicos (Gmail, SendGrid, Mailgun, SES) funcionam sem configuração; um relay com CA interna ou autoassinada falha até o [`tls_ca`](configuration.md#todos-os-ajustes) apontar para essa CA.

### `url` e `percent-encoding`

Para interpretar `smtps://user:pass@host:465`. Os dois já estão na árvore por causa do lettre, então declarar não custa compilação nova. Separar a string na mão erraria no trecho percent-encoded: uma senha com `@`, `/` ou `:` só fica inequívoca depois de decodificada.

### `encoding_rs`

É o que sustenta o recurso `encoding` do SDK. Declarar direto é o que permite `charset = utf-8` existir ao lado das páginas de código do Windows.

### `fern`

Só para o `Dispatch::into_log()` no canal de console que o SDK devolve. A versão precisa acompanhar a do próprio SDK.

## O include gerado

O `build.rs` escreve `include/email_samp.inc` a partir de `include/email_samp.inc.in`, trocando o marcador de versão, e deriva `include/email_samp_omp.inc` dele reescrevendo cada nome `email_*` no estilo do open.mp. Edite o `.inc.in`; os outros dois são saída de build e são sobrescritos.
