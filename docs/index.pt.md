# email_samp

Plugin de e-mail (SMTP) para **SA-MP** e **Open Multiplayer**, escrito em Rust.

Envia e-mail direto do servidor — código de registro, recuperação de senha, alerta para a equipe — sem script em PHP, sem serviço externo e sem plugin abandonado no meio. O plugin fala SMTP por conta própria, sobre TLS, e toda chamada retorna na hora: a mensagem sai numa thread de trabalho e responde por um callback do Pawn.

O mesmo binário carrega no SA-MP e no Open Multiplayer, como componente nativo ou pelo modo legado.

## O caso comum inteiro

```pawn
#include <a_samp>
#include <email_samp>

public OnGameModeInit()
{
    email_setup();
    email_send_to("jogador@exemplo.com", "Bem-vindo", "Sua conta esta pronta.");
    return 1;
}
```

O `email_setup()` lê a configuração do ambiente ou do `smtp.ini`; o `email_send_to` põe a mensagem na fila e volta. Nada trava o servidor.

## Por onde seguir

| Página | O que responde |
|---|---|
| [Instalação](installation.md) | Onde ficam os arquivos no SA-MP e no open.mp |
| [Configuração](configuration.md) | Onde guardar a senha, e cada ajuste disponível |
| [Envio](sending.md) | Uma linha, montar mensagem, callbacks, prioridade |
| [Modelos](templates.md) | O texto num arquivo `.tpl` ou `.html`, com valores |
| [Erros](errors.md) | Saber o que falhou, e por quê |
| [Segurança](security.md) | O que o plugin protege, e o que fica com você |
| [Referência da API](api-reference.md) | Cada native e callback |

A pasta [`examples/`](https://github.com/NullSablex/email-samp/tree/master/examples) tem um gamemode funcional por assunto. Comece pelo `00_ready_to_send.pwn`: preencha quatro linhas e ele envia.

## O que ele faz por você

- **Nada trava.** O envio corre em threads de trabalho; um relay lento nunca segura o servidor.
- **Uma tabela de ajustes**, seja qual for a fonte onde você os guarda.
- **E-mail vindo de um arquivo**, editável sem recompilar.
- **Ritmo pensado para provedores reais**: limite por minuto, prioridades, nova tentativa em falha temporária e teto de fila.
- **Seguro com o que o jogador digita**: injeção de cabeçalho recusada, HTML escapado e anexo preso à pasta do servidor.
