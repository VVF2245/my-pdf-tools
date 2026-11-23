// Synthetic notes with images: a photo on the first page, and a logo in the header of every
// page (so it must be dropped as a repeated image). The PNG files are made by the code that
// compiles this (see `fixture_assets` in the reader), they are not committed.
#set page(paper: "a4", margin: 2.5cm, header: image("assets/logo.png", width: 4cm))
#set text(font: "Libertinus Serif", size: 12pt, lang: "es")
#set par(justify: true)

= Informe con imágenes

Este documento de prueba contiene una fotografía y un logotipo repetido en todas las páginas.

#figure(image("assets/photo.png", width: 8cm), caption: [Una fotografía de ejemplo])

#for i in range(1, 40) [
  Párrafo número #i del informe, escrito solo para que el documento ocupe varias páginas y el logotipo de la cabecera aparezca en todas ellas.

]
