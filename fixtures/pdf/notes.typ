// Synthetic lecture notes: three heading levels (25 / 17 / 14.5 pt over 12 pt body),
// justified text with hyphenation, a list, a code block, a running header and page numbers.
#set page(
  paper: "a4",
  margin: 2.5cm,
  header: [
    #set text(size: 9pt)
    Apuntes de programación #h(1fr) Curso 2026
  ],
  numbering: "1",
)
#set text(font: "Libertinus Serif", size: 12pt, lang: "es", hyphenate: true)
#set par(justify: true)
#show heading.where(level: 1): set text(size: 25pt)
#show heading.where(level: 2): set text(size: 17pt)
#show heading.where(level: 3): set text(size: 14.5pt)
#show raw: set text(font: "DejaVu Sans Mono", size: 10pt)

#let parrafo = [
  La programación consiste en describir, paso a paso, cómo resolver un problema de
  manera que una computadora pueda ejecutarlo. Un programa bien estructurado es fácil de
  leer, de probar y de modificar, y por eso conviene dividirlo en funciones pequeñas
  con responsabilidades claras. Las consideraciones de eficiencia importan, pero la
  claridad suele ser el primer objetivo.
]

= Fundamentos

== Variables y tipos

#parrafo

#parrafo

=== Tipos básicos

Los tipos básicos más habituales son los números enteros, los números decimales, el
texto y los valores lógicos. Estos son algunos ejemplos:

- Enteros, como 3 o -12
- Decimales, como 2,5
- Texto, como "hola"

```python
edad = 20
nombre = "Ana"
print(nombre, edad)
```

#parrafo

= Estructuras de control

== Condicionales

#parrafo

#parrafo

== Bucles

#parrafo

#parrafo

= Funciones

== Definir y llamar

#parrafo

#parrafo

#parrafo

= Cierre

== Resumen

#parrafo

#parrafo

#parrafo

== Siguientes pasos

#parrafo

#parrafo

#parrafo
