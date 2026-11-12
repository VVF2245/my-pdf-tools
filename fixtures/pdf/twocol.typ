// Synthetic two-column article: a title over a two-column body. The content stream gives
// the whole left column first and then the right one, like LaTeX output.
#set page(paper: "a4", margin: 2cm)
#set text(font: "Libertinus Serif", size: 10pt, lang: "es")
#set par(justify: true)

#align(center)[
  #text(size: 17pt, weight: "bold")[Un artículo de ejemplo en dos columnas]
]
#v(0.6cm)

#let texto = [
  Este texto de prueba ocupa dos columnas para comprobar el orden de lectura. La
  primera columna se lee de arriba abajo y, al terminar, continúa la segunda. Un
  lector que mezclara las líneas de ambas columnas produciría frases sin sentido.
  Conviene también que el texto sea lo bastante largo para llenar varias páginas y
  que haya párrafos de distinta longitud.
]

#columns(2, gutter: 1cm)[
  #for _ in range(30) [
    #texto

  ]
]
