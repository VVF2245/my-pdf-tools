// Synthetic slide with a diagram drawn with shapes (no embedded image): slides mode renders
// the page to a picture.
#set page(width: 25.4cm, height: 14.29cm, margin: 1.5cm)
#set text(font: "Libertinus Serif", size: 20pt, lang: "es")

= Arquitectura del sistema

#grid(
  columns: 4,
  gutter: 0.8cm,
  rect(width: 4cm, height: 2cm)[Cliente],
  rect(width: 4cm, height: 2cm)[Servidor],
  rect(width: 4cm, height: 2cm)[Base de datos],
  rect(width: 4cm, height: 2cm)[Copia],
)
#v(0.5cm)
#line(length: 100%)
#circle(radius: 0.8cm)
