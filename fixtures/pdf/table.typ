// Synthetic page with a paragraph and a table (header row + numeric cells).
#set page(paper: "a4", margin: 2.5cm)
#set text(font: "Libertinus Serif", size: 11pt, lang: "es")
#set par(justify: true)

= Resultados del experimento

La siguiente tabla recoge las mediciones obtenidas en tres ensayos. Cada fila
corresponde a una muestra y las columnas indican el valor medido.

#align(center)[
  #table(
    columns: 4,
    inset: 6pt,
    align: (left, right, right, right),
    table.header[*Muestra*][*Ensayo 1*][*Ensayo 2*][*Ensayo 3*],
    [Alfa], [0,69], [0,51], [0,53],
    [Beta], [1,20], [1,18], [1,22],
    [Gamma], [3,40], [3,35], [3,41],
    [Delta], [7,01], [6,98], [7,03],
  )
]

Como se observa, los valores de cada muestra son muy parecidos entre ensayos, lo que
indica que el procedimiento es repetible.
