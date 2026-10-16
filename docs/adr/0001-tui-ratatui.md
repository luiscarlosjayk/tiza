# 0001 · CodeDraw es una TUI con ratatui

**Estado:** aceptada (2026-10-15)

## Contexto

El caso de uso es explicar código en video mientras se comparte pantalla con el editor (Zed). La herramienta debe convivir con el editor en una terminal (la integrada de Zed, Ghostty, WezTerm), arrancar al instante y poder recibir código por stdin o portapapeles. Se consideró una app de escritorio nativa con `gpui`.

## Decisión

Una TUI en Rust con **ratatui 0.30** + **crossterm 0.29**. El núcleo (`codedraw-core`) no depende de ratatui: produce un lienzo de celdas propio que la TUI copia a su buffer y que el CLI vuelca a texto/ANSI. Teclado primero, con línea de comandos `:` (estilo vim); el ratón será opt-in.

## Consecuencias

- Se hereda la fuente y el tamaño del terminal del usuario: la legibilidad en video depende de su configuración, no de la nuestra. Documentamos un perfil de grabación.
- Ancho real a 1080p con fuente grande: ~75–100 columnas. El layout debe degradar bien (ADR 0002).
- Nada de dibujo libre: el lenguaje visual es el de los caracteres de caja y las flechas Unicode.
- Inspiración declarada: Excalidraw (concepto), excalidraw/cli (CLI para humanos y agentes), herdr (API por socket), tuios (modal vim + ratón).
