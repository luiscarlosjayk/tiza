# 0002 · Anclas por (línea, grafema) y lenguaje visual

**Estado:** aceptada (2026-10-15), revisable tras el primer video

## Contexto

Las anotaciones deben sobrevivir al scroll, al cambio de tamaño del terminal y al cambio de tema. Las terminales y `unicode-width` no siempre coinciden en el ancho de emojis; las tabulaciones dependen de la configuración. En un terminal de ~75 columnas no hay margen derecho.

## Decisión

- El **modelo** ancla en coordenadas del texto: línea absoluta (1-based, se conserva al abrir `archivo.rs:12-48`) y **offset de grafema** (0-based, `unicode-segmentation`). La conversión a celdas ocurre solo al renderizar (`unicode-width`, tabs a 4).
- Para agentes y comandos, las anclas se dan por **texto** (`find`, con línea y ocurrencia opcionales) y se resuelven a grafemas al crear; el `find` original se guarda en el `.codedraw` para re-anclar en el futuro.
- Lenguaje visual de la v0:
  - **Cajas** rodean de verdad el rango: filas virtuales sin número arriba y abajo (`╭─ etiqueta ──╮` … `╰──╯`) y rieles `│` a ambos lados. Las cajas anidan.
  - **Flechas entre líneas** van por un carril a la izquierda del código: `╭──` sale de la línea origen, `╰──▶` entra en la destino; las más largas ocupan el carril más alejado.
  - **Flechas en la misma línea** usan una fila virtual debajo (`╰───▶`).
  - **Resaltes y marcas** tiñen el fondo del span. Las marcas de Rust (`move ━▶`, `borrow ╌▶`, `&mut ═▶`, `drop ╳`, `lifetime ┆`) tienen color y glifo fijos: se distinguen por forma, no solo por color.
  - **Notas y leyendas** van al margen derecho si quedan ≥ 14 celdas; si no, en una fila virtual debajo de la línea (`┆ nota`, `· leyenda`).
- Los **pasos** son un entero por anotación; se muestran las anotaciones con `step <= paso actual`.

## Consecuencias

- Las filas virtuales cambian la correspondencia fila↔línea; `Layout::row_of_line` es la única fuente de verdad para scroll y cursor.
- El ruteo de flechas no esquiva texto: cruza los rieles de caja con `─`. Suficiente para el demo; el ruteo fino es seguimiento (t-022).
- Evitamos emojis en los temas por defecto (ancho ambiguo).
