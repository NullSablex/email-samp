# Referência da API

Cada native e callback que o plugin expõe, na ordem em que o include declara. As páginas ligadas em cada seção explicam o raciocínio; aqui está a lista.

Os nomes são os em snake_case. Incluir `<email_samp_omp>` dá a mesma API na nomenclatura do open.mp: `Email_Setup`, `Email_SendTo`, `Email_SetVar`, e assim por diante.

## Callbacks

Veja [Erros](errors.md).

| | O que faz |
|---|---|
| `forward OnEmailSent(account, const recipient[], const callback[])` | Dispara em todo script carregado quando uma mensagem sai. |
| `forward OnEmailError(account, const recipient[], const callback[], const error[], errorid)` | Dispara em todo script carregado quando um envio falha de vez, depois das novas tentativas. |

## Configuracao e contas

Veja [Configuracao](configuration.md).

| | O que faz |
|---|---|
| `email_setup(const source[] = "")` | Le a configuracao e abre uma conta, em uma linha. |
| `email_connect(const host[], const user[], const password[], const settings[] = "")` | Abre uma conta com valores dados no codigo. |
| `bool:email_close(account = 0)` | Fecha uma conta, descartando as sessoes e os rascunhos presos a ela. |
| `bool:email_is_account(account = 0)` | Diz se a conta continua aberta; sem argumento, se ha e-mail configurado. |
| `bool:email_status(account, dest[], dest_len = sizeof(dest))` | Escreve `host:porta MODO` no destino, sem nenhuma credencial. |
| `bool:email_stats(account = 0, &queued = 0, &sent = 0, &failed = 0)` | Como esta a fila e o que ja passou por ela: faltam, enviados, falharam. |
| `bool:email_test(account = 0, const callback[] = "", const format[] = "", {Float,_}:...)` | Abre uma sessao, autentica e fecha, para provar que a configuracao funciona. |

## Erros

Veja [Erros](errors.md).

| | O que faz |
|---|---|
| `email_errno(account = 0)` | Codigo do ultimo erro de uma conta. |
| `bool:email_error(account, dest[], dest_len = sizeof(dest))` | Escreve a mensagem por tras do codigo, normalmente a resposta do relay. |

## Envio em uma linha

Veja [Envio](sending.md).

| | O que faz |
|---|---|
| `bool:email_send_to(const to[], const subject[], const body[], const callback[] = "", const format[] = "", {Float,_}:...)` | Manda um e-mail em texto simples para um endereco, na conta padrao. |
| `bool:email_send_file(const to[], const path[], const callback[] = "", const format[] = "", {Float,_}:...)` | Manda um arquivo `.tpl` ou `.html` para um endereco; o arquivo leva o assunto e o texto. |

## Montando uma mensagem

Veja [Envio](sending.md) e [Modelos](templates.md).

| | O que faz |
|---|---|
| `email_new(account = 0)` | Cria um rascunho de mensagem vazio. |
| `bool:email_destroy(message)` | Descarta um rascunho que nao sera enviado. |
| `bool:email_is_message(message)` | Diz se o identificador ainda e um rascunho vivo. |
| `bool:email_add_to(message, const address[], const name[] = "")` | Adiciona um destinatario visivel. |
| `bool:email_add_cc(message, const address[], const name[] = "")` | Adiciona um destinatario em copia, visivel para os demais. |
| `bool:email_add_bcc(message, const address[], const name[] = "")` | Adiciona um destinatario em copia oculta. |
| `bool:email_set_from(message, const address[], const name[] = "")` | Troca a identidade do remetente so nesta mensagem. |
| `bool:email_set_reply_to(message, const address[], const name[] = "")` | Define para onde vao as respostas. |
| `bool:email_set_subject(message, const subject[])` | Define o assunto. |
| `bool:email_set_body(message, const body[])` | Define o corpo em texto simples. |
| `bool:email_set_html(message, const html[])` | Define o corpo em HTML. |
| `bool:email_add_header(message, const name[], const value[])` | Adiciona um cabecalho que o plugin nao tem native propria. |
| `bool:email_set_priority(message, priority)` | Define o lugar da mensagem na fila de envio. |
| `bool:email_attach(message, const path[], const filename[] = "", const mime[] = "")` | Anexa um arquivo do disco. |
| `bool:email_attach_data(message, const data[], const filename[], const mime[] = "")` | Anexa texto montado no gamemode, sem passar pelo disco. |
| `bool:email_embed(message, const path[], const cid[] = "")` | Embute uma imagem para o HTML mostrar por dentro, como `cid:nome`. |
| `bool:email_set_template(message, const path[])` | Aponta a mensagem para um arquivo de modelo (`.tpl` ou `.html`). |
| `bool:email_set_var(message, const key[], const value[], {Float,_}:...)` | Define `{chave}` nesta mensagem, para qualquer tipo de valor. |
| `bool:email_send(message, const callback[] = "", const format[] = "", {Float,_}:...)` | Entrega o rascunho a uma thread de trabalho e retorna na hora. |

## Utilidades

| | O que faz |
|---|---|
| `bool:email_is_valid_address(const address[])` | Diz se o texto e um endereco valido, pelo mesmo parser do envio. |
| `email_force_reload_templates()` | Forca todo modelo em cache a ser lido do disco de novo. |
| `email_limit(limit)` | Um limite que o plugin aplica, para o gamemode conferir antes de montar. |
| `bool:email_log(level)` | Define o nivel minimo de log, em tempo de execucao. |

## env_samp

Veja [Configuracao](configuration.md#um-env-pelo-env_samp-recomendado).

| | O que faz |
|---|---|
| `email_setup_env(const prefix[] = "smtp")` | Abre uma conta a partir das chaves do `.env` lidas pelo env_samp. |

## Constantes

| Grupo | Valores |
|---|---|
| Prioridades | `EMAIL_PRIORITY_LOW`, `EMAIL_PRIORITY_NORMAL`, `EMAIL_PRIORITY_HIGH` |
| Erros | `EMAIL_ERROR_*` — veja [Erros](errors.md#os-codigos) |
| Limites | `EMAIL_LIMIT_RECIPIENTS`, `EMAIL_LIMIT_HEADERS`, `EMAIL_LIMIT_ATTACHMENT_BYTES`, `EMAIL_LIMIT_DRAFTS`, `EMAIL_LIMIT_HEADER_LEN` |
| Níveis de log | `EMAIL_LOG_NONE`, `EMAIL_LOG_ERROR`, `EMAIL_LOG_WARNING`, `EMAIL_LOG_INFO`, `EMAIL_LOG_ALL` |
| Contas | `EMAIL_EVERY_ACCOUNT`, para o `email_stats` |
| Versão | `EMAIL_SAMP_VERSION` |

`Email::Nome(args)` declara um callback em uma linha, no lugar do par `forward` + `public`.

