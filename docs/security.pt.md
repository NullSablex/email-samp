# Segurança

E-mail montado com o que o jogador digita é a superfície de ataque que interessa aqui, então o plugin confere o que dá para conferir e é explícito sobre o que não dá.

## Cabeçalhos

Um cabeçalho de e-mail termina num CR ou LF solto. Um assunto montado com `format(assunto, sizeof(assunto), "Bem-vindo, %s", nick)`, com um nick contendo `\r\nBcc: atacante@mal.tld`, não produz um assunto estranho — produz um **cabeçalho novo**, e o servidor manda cópia oculta de todo e-mail de recuperação de senha para o atacante.

Por isso, tudo que vira cabeçalho é **recusado** se contiver CR, LF ou NUL: assunto, nome de exibição, endereço, nome e valor de cabeçalho personalizado, nome de anexo e o assunto que um modelo renderizou. Recusado em vez de limpo, com `EMAIL_ERROR_HEADER_INJECTION`, para o gamemode ficar sabendo que a entrada é hostil em vez de mandar meia mensagem em silêncio.

Jogador de verdade não tem quebra de linha no nick. Trate como tentativa, não como erro de digitação.

O **corpo é isento de propósito**: quebra de linha é legítima ali, e é codificada, então o corpo nunca alcança o bloco de cabeçalhos.

## Endereços

Endereços são interpretados por um parser RFC 5321, nunca comparados com padrão de texto. O `email_is_valid_address` usa o mesmo parser, então o que ele aceita é exatamente o que não será recusado depois:

```pawn
if (!email_is_valid_address(endereco))
{
    return SendClientMessage(playerid, -1, "Esse e-mail nao e valido.");
}
```

Vale chamar no registro: um erro de digitação pego com o jogador ainda no servidor pode ser corrigido; pego na hora do envio vira uma devolução silenciosa horas depois. Não escreva uma checagem mais rígida por conta própria — `primeiro.ultimo+tag@sub.exemplo.com.br` é válido e comum.

## HTML

Valores substituídos na parte `[html]` de um modelo são escapados, então um nick com `<script>` chega como texto. Isso é automático; veja [Modelos](templates.md#escape-e-a-unica-saida).

O `email_set_html` manda a sua marcação como está — ele não tem como distinguir a sua marcação da de um jogador. Ponha texto de jogador no HTML por meio de um modelo, ou escape você mesmo.

## Arquivos

Anexo, imagem embutida e modelo podem terminar **dentro de um e-mail**, então caminho que escapa da pasta do servidor não é só leitura: é uma forma de mandar um arquivo para fora. Um gamemode montando `logs/<nick>.txt` mandaria o `server.cfg` para quem pedisse.

A regra:

- `..` é aritmética de caminho e é permitido enquanto o resultado ficar dentro da pasta do servidor;
- nenhuma parte do caminho pode ser um **atalho** (symlink, ou junction no Windows), aponte para onde apontar;
- precisa terminar num arquivo comum.

A checagem roda quando a native é chamada e de novo na thread de trabalho, logo antes de ler, então trocar o arquivo por um atalho nesse intervalo não passa. O arquivo de configuração e o `tls_ca` são isentos: só o operador os define, e nunca viram parte de um e-mail.

## TLS

O STARTTLS é **obrigatório**, nunca oportunista. Quem consegue ver a conexão também consegue remover a oferta da saudação do relay, e aí um cliente oportunista autentica em texto claro. Exigir a subida transforma esse ataque em falha de envio.

O repositório de certificados é o pacote compilado dentro do plugin, não o do sistema operacional. Provedores públicos funcionam sem configuração; um relay com CA interna ou autoassinada precisa do `tls_ca` apontando para a CA em formato PEM.

Nunca use `tls_verify = 0` para passar por um erro de certificado: isso aceita qualquer certificado, então quem conseguir redirecionar a conexão lê a senha e todas as mensagens. O plugin avisa no console toda vez que está desligado.

## Credenciais em texto claro

Mandar a senha por conexão sem criptografia para um host que **não** é esta máquina é recusado. Exige um `allow_plaintext_auth = 1` explícito, e ainda assim avisa a cada início.

Relay em `localhost`, `127.x` ou `::1` é permitido com aviso: ali a sessão nunca chega à rede.

## Credenciais e logs

Senha, endereço e resposta do relay nunca chegam ao console — o console recebe uma linha curta e um código de erro, e o detalhe vai para `logs/email.log`. Erro de configuração nomeia a chave e o que se esperava, nunca o valor.

Mantenha `smtp.ini` e `.env` fora do controle de versão. O `email_status` é seguro num comando de depuração: ele imprime `host:porta MODO` e nenhuma credencial.

## Limites

Existem para que um engano falhe na linha que o cometeu, em vez de virar problema de memória ou uma sessão derrubada pelo relay:

| Limite | Valor |
|---|---|
| Destinatários por mensagem (To + Cc + Bcc) | 100 |
| Cabeçalhos personalizados por mensagem | 50 |
| Bytes de anexo por mensagem | 25 MB |
| Rascunhos abertos em todos os scripts | 5000 |
| Tamanho de um valor de cabeçalho | 900 |

Pergunte com `email_limit(EMAIL_LIMIT_*)` em vez de escrever os números no gamemode.
