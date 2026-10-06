// Fixed experiment profile; no Carta application integration.
#let carta-classic(body) = {
  set page(
    paper: "a4",
    margin: (x: 32mm, y: 25mm),
    header: none,
    footer: context align(center, text(size: 8pt, counter(page).display("1"))),
    footer-descent: 10mm,
  )
  set text(
    font: ("Source Serif 4", "Noto Sans", "Noto Sans Hebrew",
           "Noto Sans Arabic", "Noto Sans Devanagari", "Noto Sans JP"),
    size: 11pt,
    fill: black,
    lang: "it",
    hyphenate: false,
    top-edge: 0.8em,
    bottom-edge: -0.2em,
  )
  // Explicit edges + leading give a 1.35em baseline distance.
  set par(justify: false, leading: 0.35em, spacing: 0.85em)
  set align(left)
  set heading(numbering: none)
  show heading: set text(weight: "semibold")
  show heading.where(level: 1): set text(size: 20pt)
  show heading.where(level: 2): set text(size: 15pt)
  show heading.where(level: 3): set text(size: 12pt)
  show strong: set text(weight: "semibold")
  set quote(block: true, quotes: false)
  show quote: it => block(inset: (left: 8mm), it.body)
  set list(indent: 2mm, body-indent: 5mm)
  set enum(indent: 2mm, body-indent: 5mm)
  show raw: set text(font: "Source Code Pro", size: 9pt)
  set raw(theme: none)
  body
}
