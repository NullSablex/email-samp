# Instalação

Baixe o `email_samp.so` (Linux) ou o `email_samp.dll` (Windows) e o include na [última release](https://github.com/NullSablex/email-samp/releases/latest). O plugin não depende de nada do sistema: sem PHPMailer, sem `sendmail`, sem OpenSSL.

## O include

Ponha o `email_samp.inc` em `pawno/include` (SA-MP) ou `qawno/include` (open.mp) e então:

```pawn
#include <email_samp>
```

A release traz também o `email_samp_omp.inc`, a mesma API na nomenclatura do open.mp (`Email_Setup`, `Email_SendTo`). Escolha um por script, nunca os dois.

## SA-MP

Ponha o binário em `plugins/` e declare no `server.cfg`:

```
plugins email_samp.so
```

No Windows, `plugins email_samp.dll`.

## Open Multiplayer (nativo)

Ponha o binário em `components/`. O open.mp encontra sozinho ao subir, sem nenhuma entrada no `config.json`. Esse é o caminho recomendado: o plugin passa a usar as APIs do próprio open.mp para timers e log.

## Open Multiplayer (legado)

O mesmo binário funciona como plugin legado. Ponha em `plugins/` e adicione ao `legacy_plugins` no `config.json` — esse precisa ser declarado, senão o open.mp ignora.

## Conferindo se carregou

O console mostra um banner com a versão ao subir. Para provar também que as credenciais funcionam, sem esperar o primeiro e-mail de verdade:

```pawn
public OnGameModeInit()
{
    email_setup();
    email_test(0, "OnMailTested");
    return 1;
}

Email::OnMailTested(success)
{
    printf("[email] %s", success ? ("o e-mail funciona") : ("o e-mail NAO funciona"));
    return 1;
}
```

O detalhe de qualquer falha vai para `logs/email.log`; o console recebe uma linha curta e um código de erro.

## Compilando do código-fonte

Precisa do Rust estável com os alvos `i686-unknown-linux-gnu` e `i686-pc-windows-msvc`, do `gcc-multilib`/`g++-multilib` para as partes em C de 32 bits do TLS, e do LLVM apenas quando se compila o `.dll` do Windows a partir do Linux.

```bash
./scripts/build-linux.sh              # .so do Linux + .dll do Windows
./scripts/build-windows.sh            # a partir do Windows
PROFILE=dev ./scripts/build-linux.sh  # build de desenvolvimento
```
