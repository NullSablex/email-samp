# Modelos

Um modelo é um **arquivo** com o texto do e-mail e `{marcadores}` onde entram os valores do gamemode. Edite o arquivo, salve, e o próximo e-mail já usa: sem reiniciar, sem recompilar. Não existe identificador para criar nem nada para destruir — o arquivo já tem nome.

```pawn
new msg = email_new();
email_add_to(msg, endereco, nome);
email_set_template(msg, "mails/bemvindo.tpl");
email_set_var(msg, "name", nome);
email_send(msg);
```

## Dois formatos

### `.tpl` — assunto, texto e HTML num arquivo

```ini
; Tudo antes da primeira secao e ignorado, entao este comentario pode ficar.

[subject]
Bem-vindo ao {server}, {name}!

[text]
Ola {name},

Sua conta no {server} esta pronta.

[html]
<p>Ola <b>{name}</b>, sua conta no {server} esta pronta.</p>
```

As seções são opcionais e podem vir em qualquer ordem; `[body]` e `[plain]` são apelidos de `[text]`. Escreva `{{` e `}}` para chave literal, que é o que o CSS na parte HTML precisa.

É o formato que vale a pena quando o e-mail importa: ele carrega a versão em texto simples que clientes sem HTML — e filtros de spam — esperam.

### `.html` — um arquivo HTML comum

Do tipo que você já tem, exportado de um designer ou copiado de um projeto em PHP. Sem seções, sem nada para aprender:

```html
<html>
  <head><title>Bem-vindo ao {server}, {name}!</title></head>
  <body><p>Sua conta esta pronta.</p></body>
</html>
```

O `<title>` vira o assunto. Qualquer outro arquivo sem seções é usado como corpo em texto simples.

## Valores

Uma native recebe todo tipo de valor:

```pawn
email_set_var(msg, "name",  nick);           // texto, como esta
email_set_var(msg, "slots", "%d", 200);      // um numero
email_set_var(msg, "vip",   "%d", true);     // um bool: 1 ou 0
email_set_var(msg, "saldo", "%.2f", dinheiro); // casas decimais, sua escolha
email_set_var(msg, "linha", "%s (%d)", nome, nivel);
```

!!! warning "Os especificadores não escapam nada"
    `%d`, `%s` e `%f` não são medida de segurança, e você não precisa deles para tornar um valor seguro. O escape é automático e acontece depois, quando o valor entra na parte `[html]`. O especificador só diz a uma chamada variádica que tipo de célula vem a seguir — a mesma razão pela qual o `format` precisa de um — e o `%.Nf` ainda escolhe as casas decimais.

Ou seja, **texto não precisa de especificador nenhum**. Sem nada depois do valor, ele é usado exatamente como está, e é por isso que um nick com `%` também não precisa de escape.

| Especificador | Valor |
|---|---|
| `%d`, `%i` | inteiro, ou bool (`1` / `0`) |
| `%f`, `%.Nf` | decimal; seis casas, ou `N` delas |
| `%s` | texto, quando você está juntando com outra coisa |
| `%r` | texto, **com o escape automático desligado** |
| `%%` | um sinal de porcentagem literal |

### Escape, e a única saída

Um valor substituído na parte `[html]` é escapado, então um nick com `<script>` chega como texto. A marcação vem do arquivo de modelo, que é do servidor; os valores são o que um jogador pode ter digitado.

O `%r` é a única forma de pôr marcação sua:

```pawn
email_set_var(msg, "linhas", "%r", html_que_voce_montou);  // voce montou
email_set_var(msg, "nick", nick);                          // o jogador digitou
```

Nunca use `%r` em algo que o jogador digitou: é exatamente a proteção que você estaria desligando.

### O que um marcador não consegue fazer

- Um valor **nunca é varrido de novo**. Um valor contendo `{outra_chave}` fica literal, então um nick não serve para ler o valor de outra variável.
- Um **marcador desconhecido continua visível** no e-mail, com chaves e tudo. Erro de digitação que se vê é melhor que string vazia que ninguém nota.
- O **assunto renderizado é reconferido** contra quebra de linha, já que um valor poderia trazer uma.

## A ordem não importa

O arquivo é **lido** quando você chama `email_set_template`, então caminho errado falha naquela linha. Ele é **renderizado** na hora de enviar, então `email_set_template` e `email_set_var` podem ser chamados em qualquer ordem.

O modelo preenche só o que a mensagem deixou vazio, então `email_set_subject` depois de `email_set_template` vence — que é o sentido que qualquer um espera.

## Cache e recarga

Os modelos ficam em cache por arquivo, para um mailing não reler o mesmo arquivo mil vezes. O cache percebe mudança no tamanho ou na data do arquivo, então editar o texto com o servidor rodando basta: o próximo e-mail já usa.

O `email_force_reload_templates()` cobre o que isso não pega — um deploy que copia mantendo as datas originais (`rsync -a`, `scp -p`), ou um compartilhamento de rede com relógio grosso:

```pawn
printf("[email] %d modelo(s) serao lidos de novo", email_force_reload_templates());
```

Mensagens que já estão na fila mantêm a versão com que foram montadas.
