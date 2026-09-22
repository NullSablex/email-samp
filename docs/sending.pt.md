# Envio

Toda native aqui retorna na hora. A mensagem é entregue a uma thread de trabalho e sai em segundo plano, então chamar qualquer uma delas dentro do `OnPlayerConnect` é tranquilo.

## Uma linha

```pawn
email_send_to("jogador@exemplo.com", "Bem-vindo", "Sua conta esta pronta.");
```

Ou, quando o texto mora num arquivo e não precisa de nenhum valor:

```pawn
email_send_file("jogador@exemplo.com", "mails/manutencao.html");
```

As duas usam a conta padrão e texto simples (ou o que o arquivo carregar).

## Saber se funcionou

Informe um callback. Ele recebe o resultado **primeiro**, e depois os valores que você listou após a string de formato:

```pawn
email_send_to(endereco, "Bem-vindo", corpo, "OnMailSent", "d", playerid);

Email::OnMailSent(success, playerid)
{
    SendClientMessage(playerid, -1, success ? ("E-mail enviado.") : ("Falhou."));
    return 1;
}
```

O `Email::` vem com o include e equivale a `forward OnMailSent(success, playerid);` mais `public OnMailSent(success, playerid)`. O par escrito à mão funciona igual.

As letras de formato são `d`/`i` para inteiro, `f` para decimal e `s` para texto — só para os extras, nunca para o `success` que vem primeiro.

O callback é opcional. Uma falha também é informada pelo [`OnEmailError`](errors.md), que todo script enxerga.

## Montando uma mensagem

No momento em que você precisa de HTML, anexo, mais de um destinatário ou um modelo:

```pawn
new msg = email_new();
email_add_to(msg, endereco, nome);
email_set_subject(msg, "Relatorio semanal");
email_set_body(msg, "Relatorio em anexo.");
email_set_html(msg, "<h2>Relatorio semanal</h2><p>Em anexo.</p>");
email_attach(msg, "scriptfiles/relatorio.csv");
email_send(msg, "OnMailSent", "d", playerid);
```

Definir corpo em texto **e** em HTML produz uma mensagem `multipart/alternative`: clientes que mostram HTML usam o HTML, os demais caem no texto. Mandar só HTML pontua mal nos filtros de spam, então escreva os dois — ou use um [modelo `.tpl`](templates.md), que mantém os dois juntos.

O `email_send` **consome o identificador** assim que a mensagem entra na fila: depois disso o id está morto. Se ele devolver `false`, a mensagem ainda é sua — envie depois, ou destrua com `email_destroy`.

### Destinatários

`email_add_to` é visível, `email_add_cc` é visível para todos os outros e `email_add_bcc` é oculto. Para mandar a vários jogadores tem que ser Bcc, ou cada jogador fica sabendo o e-mail de todos os outros. O teto é 100 somando os três; pergunte com `email_limit(EMAIL_LIMIT_RECIPIENTS)` em vez de escrever o número.

### Anexos e imagens

| Native | O que faz |
|---|---|
| `email_attach` | arquivo do disco, oferecido para baixar |
| `email_attach_data` | texto montado no gamemode (um CSV), sem arquivo |
| `email_embed` | imagem mostrada **dentro** do HTML, como `cid:nome` |

Caminho, tamanho e tipo MIME são conferidos na linha que anexa, então um engano não espera o envio. O conteúdo é lido pela thread de trabalho na hora de enviar, então arquivo grande nunca trava o servidor. Veja em [Segurança](security.md#arquivos) onde os arquivos podem estar.

```pawn
email_embed(msg, "scriptfiles/logo.png");
email_set_html(msg, "<img src=\"cid:logo\"> Bem-vindo!");
```

## Prioridade

```pawn
email_set_priority(msg, EMAIL_PRIORITY_HIGH);
```

| Nível | Para |
|---|---|
| `EMAIL_PRIORITY_HIGH` | tem jogador esperando: código de confirmação, recuperação de senha |
| `EMAIL_PRIORITY_NORMAL` | o padrão |
| `EMAIL_PRIORITY_LOW` | mailing, informativo — ninguém está esperando |

Cada nível é uma **vantagem de tempo**, não uma fila separada: o `HIGH` passa na frente de um `NORMAL` enfileirado no mesmo instante, mas um `LOW` que esperou dez minutos passa na frente de um `NORMAL` recém-chegado. Assim a prioridade reordena a fila sem deixar nada esperando para sempre.

O `LOW` também só pode ocupar três quartos da fila, para um mailing nunca bloquear uma recuperação de senha, e o `HIGH` nunca é recusado por fila cheia.

## Ritmo

Três ajustes fazem o trabalho que o gamemode faria com timers:

- **`rate_limit`** — mensagens por minuto, espaçadas por igual (provedores olham janelas curtas, e a cota inteira no primeiro segundo é exatamente o que eles estrangulam). Segura só a própria conta.
- **`retries`** — falha temporária (um 4xx, ou o relay inalcançável) é tentada de novo com espera dobrando. Seu callback roda **uma vez**, com o resultado final. Quando o próprio relay está fora do ar, a conta inteira é pausada, então uma queda custa uma sondagem por pausa em vez de uma por mensagem na fila.
- **`queue_limit`** — passando dele, o envio falha na linha que chamou, com `EMAIL_ERROR_QUEUE_FULL`, em vez de a fila crescer sem fim.

Assim um mailing é um laço simples:

```pawn
for (new i = 0; i < MAX_PLAYERS; i++)
{
    new endereco[128];
    if (!GetPlayerEmail(i, endereco)) continue;

    new msg = email_new();
    email_add_to(msg, endereco);
    email_set_subject(msg, "Aviso");
    email_set_body(msg, texto);
    email_set_priority(msg, EMAIL_PRIORITY_LOW);

    if (email_send(msg)) continue;

    email_destroy(msg);
    if (email_errno() == EMAIL_ERROR_QUEUE_FULL) break;   // o resto na proxima
}
```

## Acompanhando a fila

```pawn
new faltam, enviados, falharam;
email_stats(EMAIL_EVERY_ACCOUNT, faltam, enviados, falharam);
```

`faltam` é o que ainda tem que sair, incluindo mensagens em nova tentativa. Passe o id de uma conta, ou `0` para a padrão, para ver só aquela.

As três saídas têm valor padrão, então dá para pedir só a primeira ou as duas primeiras — `email_stats(0, faltam)` é chamada válida. O `0` da declaração é só o conteúdo inicial da célula escondida; o plugin escreve por cima.

## Desligamento

Quando o servidor para, o plugin espera até dez segundos a fila esvaziar e depois registra quantas se perderam. Nada fica salvo em disco, então não comece um mailing logo antes de reiniciar.
