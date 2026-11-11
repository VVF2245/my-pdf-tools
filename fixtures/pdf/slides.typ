// Synthetic slide deck. Like many real decks, the title and the bullets are close in
// size (20 pt vs 18 pt): titles must be told apart by position, not by size.
#set page(
  width: 25.4cm,
  height: 14.29cm,
  margin: (x: 1.5cm, top: 1.2cm, bottom: 1.8cm),
  footer: context [
    #set text(size: 9pt)
    Bases de datos · Curso 2026 #h(1fr) #counter(page).display()
  ],
)
#set text(font: "Libertinus Serif", size: 18pt, lang: "es")

#let slide(title, body) = {
  pagebreak(weak: true)
  text(size: 20pt, weight: "bold")[#title]
  v(0.8cm)
  body
}

#text(size: 28pt, weight: "bold")[Introducción a las bases de datos]
#v(0.5cm)
Curso de ejemplo para pruebas

#slide("¿Qué es una base de datos?")[
  - Colección organizada de datos
  - Acceso rápido y concurrente
    - Varios usuarios a la vez
    - Control de permisos
  - Gestionada por un sistema gestor
]

#slide("El modelo relacional")[
  - Datos en tablas con filas y columnas
  - Cada fila se identifica con una clave primaria
  - Las tablas se relacionan mediante claves ajenas
  - Una relación es un conjunto de filas sin orden ni duplicados, y cada columna tiene un nombre y un tipo de dato fijo que todas las filas deben respetar sin excepción alguna dentro de la misma tabla
    - Esto permite comprobar de forma automática que los datos guardados son coherentes, y rechazar los que no cumplan las reglas definidas por quien diseñó la base de datos
]

#slide("Consultas con SQL")[
  - Lenguaje declarativo estándar
  - Operaciones básicas:
    - Seleccionar
    - Insertar
    - Actualizar
    - Borrar
]

#slide("Resumen")[
  - Los datos se guardan en tablas
  - Las claves relacionan las tablas
  - SQL consulta y modifica los datos
]

// The same title on several slides: it must survive (titles are not running headers).
#slide("Ejemplo")[
  - Una consulta sencilla
  - Sobre una sola tabla
]

#slide("Ejemplo")[
  - Una consulta con filtro
  - Con una condición
]

#slide("Ejemplo")[
  - Una consulta con unión
  - Entre dos tablas
]
