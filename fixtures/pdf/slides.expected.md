## Introducción a las bases de datos

Curso de ejemplo para pruebas

## ¿Qué es una base de datos?

- Colección organizada de datos
- Acceso rápido y concurrente
  - Varios usuarios a la vez
  - Control de permisos
- Gestionada por un sistema gestor

## El modelo relacional

- Datos en tablas con filas y columnas
- Cada fila se identifica con una clave primaria
- Las tablas se relacionan mediante claves ajenas
- Una relación es un conjunto de filas sin orden ni duplicados, y cada columna tiene un nombre y un tipo de dato fijo que todas las filas deben respetar sin excepción alguna dentro de la misma tabla
  - Esto permite comprobar de forma automática que los datos guardados son coherentes, y rechazar los que no cumplan las reglas definidas por quien diseñó la base de datos

## Consultas con SQL

- Lenguaje declarativo estándar
- Operaciones básicas:
  - Seleccionar
  - Insertar
  - Actualizar
  - Borrar

## Resumen

- Los datos se guardan en tablas
- Las claves relacionan las tablas
- SQL consulta y modifica los datos

## Ejemplo

- Una consulta sencilla
- Sobre una sola tabla

## Ejemplo

- Una consulta con filtro
- Con una condición

## Ejemplo

- Una consulta con unión
- Entre dos tablas
