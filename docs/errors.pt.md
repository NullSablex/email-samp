# Erros

Um problema aparece em um de dois momentos.

## Na hora

A native devolve `false` ou `0` — endereço malformado, nenhuma conta aberta, fila cheia, arquivo ilegível. Pergunte o que houve:

```pawn
if (!email_send_to(endereco, "Ola", corpo))
{
    switch (email_errno())
    {
        case EMAIL_ERROR_INVALID_ADDRESS:
            SendClientMessage(playerid, -1, "Esse e-mail nao e valido.");
        case EMAIL_ERROR_QUEUE_FULL:
            SendClientMessage(playerid, -1, "O e-mail esta ocupado, tente em um minuto.");
        default:
            SendClientMessage(playerid, -1, "O e-mail esta indisponivel agora.");
    }
}
```

O `email_error(conta, destino)` escreve a mensagem por trás do código, que normalmente é a resposta do próprio relay.

O `email_errno(0)` lê o **espaço global**: falhas anteriores à existência de uma conta, como arquivo de configuração ilegível ou remetente malformado.

## Depois, quando o relay responde

O `OnEmailError` roda em todo script carregado, e o seu callback de envio roda com `success = 0`. Use o callback para avisar o jogador e este para registrar o motivo:

```pawn
public OnEmailError(account, const recipient[], const callback[], const error[], errorid)
{
    printf("[email] conta %d, <%s>, erro %d: %s", account, recipient, errorid, error);
    return 1;
}
```

Ele dispara **uma vez**, depois das novas tentativas: uma falha temporária é retentada antes de você ficar sabendo, então um erro aqui é definitivo.

O `OnEmailSent` é o correspondente, para cada mensagem que sai — útil para um filterscript manter o registro sem o gamemode passar callback em cada envio.

Os dois são transmitidos, mas só para os scripts que **definem** o callback: quais são eles é descoberto uma vez, quando cada script carrega, e não a cada mensagem. Um gamemode com uma dúzia de filterscripts não custa nada pelos que não definem.

O que você escrever ali roda na thread principal, uma vez por mensagem. Deixe como uma linha de log; um mailing de cinco mil executa cinco mil vezes.

## Os códigos

| Código | Significa |
|---|---|
| `EMAIL_ERROR_NONE` | sem erro |
| `EMAIL_ERROR_INVALID_ACCOUNT` | a conta nunca foi aberta, ou está fechada |
| `EMAIL_ERROR_INVALID_MESSAGE` | a mensagem é desconhecida, ou já foi enviada |
| `EMAIL_ERROR_TEMPLATE_FAILED` | o arquivo de modelo não pôde ser lido |
| `EMAIL_ERROR_INVALID_ADDRESS` | não é um endereço válido |
| `EMAIL_ERROR_CONNECTION_FAILED` | inalcançável, recusado, tempo esgotado ou falha de TLS |
| `EMAIL_ERROR_AUTH_FAILED` | credenciais erradas, ou nenhum mecanismo em comum |
| `EMAIL_ERROR_SEND_FAILED` | o relay aceitou a sessão e recusou a mensagem |
| `EMAIL_ERROR_BUILD_FAILED` | a mensagem não pôde ser montada, ou bateu num limite |
| `EMAIL_ERROR_ATTACHMENT_FAILED` | anexo inutilizável: ilegível, grande demais, tipo MIME inválido |
| `EMAIL_ERROR_HEADER_INJECTION` | quebra de linha num valor que vira cabeçalho |
| `EMAIL_ERROR_CONFIG_FAILED` | configuração ausente, ilegível ou errada |
| `EMAIL_ERROR_QUEUE_FULL` | a fila da conta está cheia; a mensagem **não** foi consumida |

## O que vai para onde

O console recebe uma linha curta com o código. Todo o resto — o endereço do destinatário, o texto do relay — vai só para `logs/email.log`, que rotaciona em 50 MB para arquivos `.gz`. O `email_log(EMAIL_LOG_WARNING)` muda em tempo de execução o quanto cada um recebe.

Essa separação é proposital: console é lido por quem estiver por perto, e uma mensagem de devolução costuma conter o endereço de um jogador.

## Conferindo a configuração ao subir

O `email_setup` não disca nada — ele monta o transporte, e o primeiro TCP + TLS + AUTH acontece no primeiro envio. Para descobrir antes:

```pawn
email_test(0, "OnMailTested");

Email::OnMailTested(success)
{
    new status[128];
    email_status(0, status);   // "smtp.gmail.com:587 STARTTLS"
    printf("[email] %s: %s", status, success ? ("funcionando") : ("NAO funciona"));
    return 1;
}
```

O `email_test` abre uma sessão, autentica e fecha, numa thread de trabalho como todo envio. Uma senha errada aparece no console enquanto você está olhando, em vez de no registro do primeiro jogador.
