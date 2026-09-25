# Configuração

Toda fonte aceita **as mesmas chaves**. Maiúsculas não importam e um prefixo `SMTP_` opcional é descartado, então `SMTP_HOST`, `smtp_host` e `host` são a mesma chave. Chave que o plugin não conhece é **erro**, nunca silêncio: `SMTP_PASSWD` pareceria exatamente com "sem senha configurada", e você passaria a noite atrás de um `535` que não tinha nada a ver com a senha.

## Onde guardar

### Um `.env`, pelo env_samp (recomendado)

Inclua o [env_samp](https://github.com/NullSablex/env-samp) **antes** deste plugin e o include ganha uma função:

```pawn
#include <env_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup_env();
    return 1;
}
```

com as chaves sob um prefixo, ao lado do resto dos ajustes do servidor:

```
smtp.host      = smtp.gmail.com
smtp.user      = bot@gmail.com
smtp.password  = "abcd efgh ijkl mnop"
smtp.from_name = Los Santos RP
```

Uma segunda conta é só outro prefixo: `email_setup_env("smtp.staff")` lê `smtp.staff.host` e assim por diante.

Este plugin **não** lê `.env` por conta própria, de propósito. O env_samp já lê, e dois interpretadores do mesmo formato acabariam discordando sobre uma senha com `#` no meio.

### Variáveis de ambiente

Docker, systemd ou CI definem, e o `email_setup()` puro encontra:

```yaml
environment:
  SMTP_URL: smtps://bot%40exemplo.com:segredo@smtp.gmail.com
```

### Uma URL

Uma string carrega host, porta, credenciais e criptografia, e os parâmetros de consulta carregam o resto:

```pawn
email_setup("smtps://bot%40exemplo.com:segredo@smtp.gmail.com");
email_setup("smtp://user:pw@relay.local:2525/?tls=none&from_name=Los+Santos+RP");
```

O trecho de usuário e senha é percent-encoded, e é isso que torna inequívoca uma senha com `@`, `/` ou `:`. `%40` é `@`.

### `smtp.ini`

Ao lado do `server.cfg`, para quem não quer nem um nem outro. O `email_setup()` recorre a ele; `email_setup("config/mail.ini")` aponta outro caminho.

### No código

```pawn
email_connect("smtp.exemplo.com", "alertas@exemplo.com", "segredo",
    "encryption=tls; from_name=Alertas do Servidor; rate_limit=30");
```

As credenciais são argumentos separados, então a senha não precisa de escape; o resto usa as mesmas chaves, separadas por `;`.

## Ordem de descoberta

O `email_setup()` sem argumento tenta primeiro as **variáveis de ambiente** e depois o **`smtp.ini`**. O ambiente vence, para que o `SMTP_URL` de um contêiner não seja sobrescrito por um `smtp.ini` que ficou embutido na imagem. O console diz qual fonte foi usada.

## Todos os ajustes

| Chave | Significado | Padrão |
|---|---|---|
| `url` | `smtps://user:pass@host:465` — carrega tudo de uma vez | |
| `host` | endereço do relay (obrigatório, salvo com `url`) | |
| `user` | usuário SMTP (apelido `username`) | |
| `password` | senha SMTP (apelido `pass`) | |
| `port` | | segue a criptografia |
| `encryption` | `none`, `starttls` ou `tls` | `starttls` |
| `from` | endereço do remetente | o usuário |
| `from_name` | nome que o destinatário vê | |
| `helo` | nome anunciado no EHLO | o host da máquina |
| `timeout` | segundos | `30` |
| `pool_size` | sessões SMTP reaproveitadas | `4` |
| `tls_ca` | certificado raiz em PEM, para CA própria | |
| `tls_verify` | conferir o certificado do relay | `1` |
| `retries` | tentativas extras após falha **temporária** | `2` |
| `rate_limit` | mensagens por minuto, espaçadas; `0` é sem limite | `0` |
| `queue_limit` | mensagens esperando ao mesmo tempo | `1000` |
| `charset` | como as strings de 8 bits do Pawn são lidas; qualquer nome de codificação | `windows-1252` |
| `allow_plaintext_auth` | mandar a senha sem criptografia para host remoto | `0` |
| `dry_run` | escrever em `logs/dry-run/` e não enviar nada | `0` |

Booleanos aceitam `1`/`0`, `true`/`false`, `yes`/`no`, `on`/`off`.

### charset

O SA-MP não tem noção de UTF-8: um nick com acento é um byte na página de código do servidor. Ler pela página errada é o que transforma *João* em `Jo?o` ou `JoÃ£o` no e-mail.

Qualquer nome de codificação funciona, com os apelidos de sempre: `windows-1252` (o padrão, e o que o SA-MP usa na maior parte do mundo), `windows-1251` para cirílico, `windows-1250`, `1253`, `1254`, `1256`, `1257`, `iso-8859-2`, ou `utf-8` para gamemode que já guarda UTF-8. Nome desconhecido é recusado ao subir, dizendo qual chave. O ajuste vale para o processo inteiro.

No caminho inverso — a resposta do relay escrita de volta num buffer do Pawn — a conversão pode perder caracteres que a página de código não comporta, sendo o caso óbvio uma mensagem em cirílico num servidor Windows-1252. O plugin avisa no console quando isso acontece, e o texto completo fica em `logs/email.log`.

### rate_limit, retries e queue_limit

São eles que tornam seguro escrever um mailing como um laço simples; a página de [Envio](sending.md#ritmo) explica como conversam com as prioridades.

### dry_run

Escreve cada mensagem em `logs/dry-run/` como arquivo `.eml` e não envia nada — sem gastar cota e sem mandar e-mail para jogador de verdade sem querer. O arquivo é o e-mail real, com cabeçalhos e anexos, então abre em qualquer cliente.

## Provedores

| Provedor | Host | Observação |
|---|---|---|
| Gmail / Workspace | `smtp.gmail.com` | exige **senha de app**, não a senha da conta; use aspas para manter os espaços |
| SendGrid | `smtp.sendgrid.net` | o usuário é a palavra `apikey` |
| Mailgun | `smtp.mailgun.org` | usuário é `postmaster@mg.seudominio.com` |
| Amazon SES | `email-smtp.<regiao>.amazonaws.com` | credenciais SMTP do console do SES, não suas chaves da AWS |
| Postfix local | `127.0.0.1` | o único uso honesto de `encryption=none` |

Porta e criptografia se deduzem uma da outra, então definir `host` e as credenciais costuma bastar.

## Várias contas

A **primeira** conta aberta vira a padrão: toda native que recebe conta usa ela quando você não passa nada, e um filterscript que abre a própria conta não consegue redirecionar o e-mail do gamemode. Qualquer outra conta é endereçada pelo id que ela devolveu.

```pawn
new Mail[2];
Mail[0] = email_setup_env();              // jogadores
Mail[1] = email_setup_env("smtp.staff");  // alertas, outro remetente

new msg = email_new(Mail[1]);
```
