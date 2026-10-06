# Typst 0.15.1: Prova Rapida Carta Classic

2026-10-06, branch `opencode/v0.2`, Debian 13 nella Distrobox `carta-dev`.
Solo esperimento: nessuna modifica a core, Cargo principale o `main`.

## 1. Versione E Installazione

Binario ufficiale `typst-x86_64-unknown-linux-musl.tar.xz` dalla release
`https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-unknown-linux-musl.tar.xz`,
estratto in `/opt/carta-pdf-bench/typst-0.15.1/` dentro il container.
Versione verificata: **`typst 0.15.1 (9dfd3a08)`**. Nessun nuovo pacchetto apt,
nessuna compilazione, installazione sull'host o uso di Nix.

Riutilizzati esattamente i nove TTF precedenti; nessun download di font.
Compile con `--font-path ../fonts --ignore-system-fonts --ignore-embedded-fonts`:
la lista font contiene solo Source Serif 4, Source Code Pro e i cinque Noto.
Il template `typst/carta-classic.typ` e' di 36 righe. Un adattatore limitato ai
costrutti del corpus emette testo letterale, non esegue contenuto come codice.
Nessun package Typst esterno. Archivio, versione, font list, byte e `ldd`:
`typst-packaging.txt`.

## 2. Resa Latin

**Tre pagine, 45.162 byte**, contro groff Latin tre pagine / 61.196 byte.
Stesso contenuto di `corpus/latin.md`; nessun taglio o page break manuale.
A4, margini 32/25 mm, 11 pt; distanza fra baseline misurata **14,85 pt**,
sinistra non giustificato. H1/H2/H3 20/15/12 pt Semibold, codice 9 pt mono.

Ispezionate tutte le pagine PNG a 120 dpi e confrontate con groff:

- Qualita' dei caratteri e leggibilita' **almeno paragonabili a groff**.
- Ritmo piu' compatto fra blocchi, a parita' di interlinea; groff e' piu' arioso.
  Non attribuisco questa preferenza estetica a una superiorita' universale.
- Semibold reale, corsivo, liste, citazione rientrata e codice funzionano.
  `pdffonts` conferma Regular, Italic, Semibold e Source Code Pro.
- **Nessun titolo orfano nel corpus**: "Il ritmo della pagina" resta col suo
  paragrafo in pagina 1; "Continuita' e leggibilita'" col suo in pagina 2.
  Risolve quindi i due casi osservati con printpdf, senza regole di break custom.
- Numeri 1/2/3 piccoli, centrati in basso, senza sovrapposizione al testo.
- La pagina 3 inizia con due righe del paragrafo precedente: break ragionevole,
  non prova esaustiva di tutte le condizioni vedove/orfane.
- **La sequenza patologicamente lunga deborda**, come negli altri due motori:
  con sillabazione disabilitata non viene spezzata automaticamente. Resta
  visibile nel PDF/PNG; non e' stata riparata per abbellire la prova.

## 3. Resa Unicode

**Una pagina, 44.541 byte**. Stesso `corpus/unicode.md`, fallback tramite lista
di famiglie nel template, senza segmentazione manuale per script.
Nessun glifo mancante visibile, compreso `日`. Ebraico e arabo hanno ordine
visivo corretto nei campioni; arabo con joining contestuale. Devanagari e
combining marks coerenti col riferimento RAQM gia' presente nel benchmark.
Greco/cirillico e accenti europei corretti. Le due forme di e-acuta sono
visivamente equivalenti; l'estrazione mantiene precomposto e decomposto.
Il documento mantiene l'allineamento sinistro italiano: non e' una prova di
auto-detection della direzione dominante per documenti interamente RTL.

`qpdf --check` passa per Latin e Unicode. Font **incorporati, subsettati e con
ToUnicode**, tutti `emb=yes sub=yes uni=yes`; nessuna sostituzione inattesa.
PDF tagged di default; nessuna certificazione PDF/UA o PDF/A effettuata.

**Estrazione: buona ma non perfetta, dipende dal modo Poppler.**
`pdftotext -layout` preserva tutti i campioni separati, incluse le frasi
ebraica/araba nell'ordine logico. Nel paragrafo misto perde pero' lo spazio
arabo/Hindi: dopo rimozione dei controlli bidi torna
`Italiano 日本語 Ελληνικά العربيةहिन्दी`. Con `-raw` il paragrafo misto e'
esatto, spazio incluso, ma le due frasi RTL separate hanno le **parole in
ordine invertito** (`עולם שלום`, `بالعالم مرحبا`). Le lettere nei singoli
termini restano corrette. Quindi non si promette copia/ricerca identica in ogni
reader: causa precisa di questa differenza non isolata nella prova rapida.
Controlli, output originali e pass/fail: `typst-extraction-checks.json`,
`typst-unicode.layout.txt`, `typst-unicode.raw.txt`.

## 4. Documenti Lunghi

Unica misura per input con **`/usr/bin/time -v`**, rendering CLI soltanto:
conversione input, verifiche e PNG esclusi. Stesso paragrafo Latin del vecchio
test, ripetuto 500/2.500 volte, flusso continuo senza interruzioni forzate.
Numero reale di pagine diverso dal nominale, nell'ordine di grandezza richiesto.

| Scala nominale | Pagine reali | Wall | Peak RSS | Byte PDF |
| --- | ---: | ---: | ---: | ---: |
| circa 100 | **79** | **1,10 s** | **113.536 KiB / 110,9 MiB** | **258.684** |
| circa 500 | **395** | **5,32 s** | **483.856 KiB / 472,5 MiB** | **1.288.680** |

Tempo e memoria crescono circa linearmente in questa prova. Nessuno swap,
exit 0 per entrambi. Dati grezzi: `typst-long-100.time.txt`,
`typst-long-500.time.txt`; pagine da `pdfinfo`, non stimate dal testo.
Sul medesimo paragrafo il precedente test nominale 100 dava groff 82 pagine,
2,69 s/~38 MiB max processo; printpdf 100 pagine, 1,00 s/~240 MiB.
Confronto indicativo, non nuova statistica: cache, tempi diversi e pipeline
groff non misurata come somma RSS. Typst e' rapido e consuma meno di printpdf
a scala 100, ma **non ha la memoria quasi costante di groff**.

## 5. Dimensione E Runtime

Binario ufficiale: **55.739.488 byte / 53,16 MiB**.
`ldd`: **statically linked**; `readelf` non mostra un interprete ELF.
Nessuna libreria dinamica da distribuire per questa build Linux musl.
Font originali: **7.755.112 byte / 7,40 MiB**; set occidentale 932.076 byte.
Binario + nove font: **63.494.600 byte / 60,55 MiB**, piu' template e licenze.
Piu' grande del piccolo eseguibile printpdf precedente, ma packaging piu'
semplice del runtime composto groff/Perl/font convertiti: CLI statico + font
TTF + template, tutto locale e senza servizi. Il binario contiene anche font
upstream inutilizzati nella prova; non e' una build custom minimizzata.
Distribuzione su macOS/altre architetture e sandboxing non testati.

## 6. Problemi E Limiti

Parola lunghissima in overflow; differenze di estrazione bidi fra `-layout`
e `-raw`; quasi mezzo GiB a circa 400 pagine. Una sola esecuzione per scala,
un corpus ridotto, nessuna suite di conformance Unicode o prova di embedding
Rust. Nessun errore/warning Typst nei render riusciti: questo non basta da
solo a garantire correttezza. Font fallback non sostituisce l'impostazione di
lingua/direzione dominante quando un documento futuro la richiedesse.

## 7. Verdetto

**Meglio di groff?** Nel complesso si': Latin almeno equivalente, titoli e
footer gia' corretti, Unicode nettamente migliore e font TTF senza conversioni.
Non meglio per memoria su documenti lunghi o dimensione del solo compositor.

**Meglio di printpdf 0.12.8?** Si' per il profilo Classic osservato: niente
difetti Semibold/footer/titoli orfani, subset standard e meno RSS a scala 100.
Shaping buono come printpdf nei campioni; estrazione RTL non perfetta neanche
qui. Il bundle CLI e' sensibilmente piu' grande.

**Abbastanza leggero per Carta?** Si' per export occasionale desktop; non
"minuscolo". Circa 5 secondi e 473 MiB a 395 pagine sono accettabili come job
separato, ma da considerare su macchine con poca RAM. Packaging statico ~63 MB
con i font completi e' ragionevole, non gratuito.

**Vale la pena come backend PDF?** **Si': ora e' il candidato piu' convincente
dei tre per qualita' complessiva e semplicita' del percorso di pubblicazione.**
Prima dell'adozione restano gestione delle parole patologiche, test copia RTL
nei reader e valutazione del budget RAM/binario. Nessun backend e' stato
iniziato: questa prova termina qui.

Output: `results/typst-latin.pdf`, `results/typst-unicode.pdf` e
`results/preview/typst-{latin,unicode}-*.png`, sotto `experiments/pdf-export/`.
Riproduzione: dentro `carta-dev`, eseguire `python3 typst/prepare.py` e
`python3 typst/quick.py` dalla directory dell'esperimento (il secondo rimisura).
