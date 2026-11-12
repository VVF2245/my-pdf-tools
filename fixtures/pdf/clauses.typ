// Synthetic legal text in two columns, like an official gazette: numbered clauses whose
// first line is indented and whose other lines start at the column edge (no hanging
// indent), and an ordinal written with a raised letter.
#set page(paper: "a4", margin: (x: 1.5cm, y: 2cm))
#set text(font: "Libertinus Serif", size: 11pt, lang: "es")
#set par(justify: true, first-line-indent: 1em, spacing: 0.9em, leading: 0.45em)

#columns(2, gutter: 0.8cm)[
  Artículo 1.

  1\. El número total de créditos establecido en los planes de estudios para cada curso académico será de sesenta y se distribuirá entre las materias del plan.

  2\. El número de créditos de cada titulación será distribuido entre la totalidad de las materias integradas en el plan de estudios que deba cursar el alumno, en función del número total de horas que comporte para el alumno la superación o realización de cada una de ellas.

  3\. En la asignación de créditos a cada una de las materias se computará el número de horas de trabajo requeridas para la adquisición de los conocimientos, capacidades y destrezas correspondientes.

  Disposición final primera.

  Este real decreto se dicta al amparo de lo dispuesto en el artículo 149.1.30.#super[a] de la Constitución española y de acuerdo con lo establecido en el artículo 87, en el artículo 88.1 y 3 y en la disposición final tercera de la Ley Orgánica de Universidades.

  Disposición final segunda.

  Corresponde al Ministro de Educación dictar, en el ámbito de sus atribuciones, las disposiciones que sean necesarias para la aplicación de lo dispuesto en este real decreto y para el desarrollo de sus normas.
]
