# Carta Space: Confronto PDF Isolato

Data: 2026-10-06. Motori: **GNU groff/gropdf 1.24.1** e **printpdf 0.12.8**.
Nessuna integrazione in Carta. Nessuna modifica al core, ai manifest principali,
alla configurazione NixOS o al branch `main`. Tutte le installazioni e le
esecuzioni sono avvenute nella Distrobox `carta-dev`.

**Sintesi:** consiglierei printpdf come candidato per Carta, ma non considererei
la versione 0.12.8 pronta da integrare senza risolvere/verificare alcuni difetti
di pubblicazione. Nei campioni supera nettamente groff per shaping, RTL,
fallback e mapping Unicode, ed e' piu' semplice da distribuire. Groff offre una
composizione occidentale piu' convincente e memoria quasi costante, ma non una
soluzione robusta per esportare documenti UTF-8 multilingue moderni.

## 1. Ambiente E Versioni

| Componente | Versione esatta |
| --- | --- |
| Distrobox | `carta-dev`, immagine `debian:stable`, Debian 13.7 trixie |
| Architettura | Linux x86_64; kernel condiviso host 6.18.51 |
| CPU | AMD Ryzen 5 PRO 3500U, 4 core / 8 thread |
| RAM | circa 13 GiB; nessun limite di memoria imposto al benchmark |
| GNU groff | 1.24.1 |
| GNU gropdf | 1.24.1 |
| printpdf | esattamente 0.12.8, pin `=0.12.8`, lockfile locale |
| rustc usato | 1.94.0 (`4a4ef493e`, 2026-03-02) |
| cargo usato | 1.94.0 (`85eff7c80`, 2026-01-15) |
| rustc/cargo Debian preesistenti | 1.85.1, non usati per printpdf |
| FontForge | 20230101, pacchetto `1:20230101~dfsg-4+b1` |
| Poppler | 25.03.0, pacchetto `25.03.0-5+deb13u4` |
| qpdf | 12.2.0 |
| GNU time | 1.9 |
| FontTools | 4.57.0 |
| Perl | 5.40.1 |

La toolchain Rust supera il minimo 1.88 di printpdf. Rustup, Cargo home,
Rustup home e target sono in `/opt/carta-pdf-bench` **dentro il container**;
nessun profilo dell'host o della home condivisa e' stato alterato.
Le versioni groff/gropdf sono richiamate tramite percorsi assoluti del prefisso
isolato, senza sostituire il groff Debian 1.23.0 preesistente.

Evidenze: `environment.txt`, `printpdf/logs/toolchain-tree.log` e
`groff/logs/facts.log`, con i percorsi dei log relativi alla directory esperimento.

## 2. Dipendenze Installate

Nuovi pacchetti richiesti direttamente per questa prova: `build-essential`,
`pkg-config`, `fontforge`, `qpdf`, `bison`, `flex`, `texinfo`,
`libuchardet-dev`, `python3-fonttools`, poi `strace` per le dipendenze runtime.
`curl`, `ca-certificates`, `poppler-utils`, `time`, `wget` e `unzip` erano gia'
presenti. **Non** e' stato installato nuovamente il vecchio Rust Debian.

La prima transazione apt ha aggiunto 89 pacchetti, incluse dipendenze e
raccomandazioni, scaricando 72,9 MB e dichiarando circa 310 MB di occupazione.
`strace` ha aggiunto anche `libunwind8`. L'elenco esatto dei nuovi pacchetti
con versioni e indicazione `automatic` e' in `apt-installed.txt`.
Tra le dipendenze transitive ci sono Pillow 11.1.0 e RAQM 0.10.2, usati
esclusivamente per un riferimento PNG di shaping con gli stessi font.

La transazione apt normale ha portato anche strumenti/librerie Python non
necessari al runtime Carta. Questo e' costo del laboratorio, **non** una
dipendenza da attribuire al renderer distribuito. Nessun uso di LaTeX, Typst,
Chromium, wkhtmltopdf o Pandoc per produrre gli output; le precedenti
installazioni nella Distrobox non sono state usate come renderer.

## 3. Implementazione Groff

Pipeline: Markdown limitato del corpus -> roff semantico -> `carta.tmac` ->
`groff -Kutf8 -Tpdf` -> gropdf. Il converter non contiene le misure del layout.
Il piccolo macro package centralizza pagina, font, titoli, paragrafi, liste,
citazioni, codice e footer. La sillabazione e' disabilitata, il testo allineato
a sinistra e non giustificato. Non e' implementato un sistema di vedove/orfane;
il macro dei titoli riserva spazio con `.ne 90p`.

Tarball GNU 1.24.1 scaricato dal mirror GNU `mirrors.kernel.org/gnu/groff` dopo
timeout di ftp.gnu.org/ftpmirror.gnu.org. SHA-256 osservato:
`74e2819795b6aff431aeac983d63a9c8968eeaba2a2eba7df8ba4c7b41e7cfd8`.
Il digest e' fissato nello script; la firma della release non e' stata verificata
indipendentemente. Compilazione e installazione in
`/opt/carta-pdf-bench/groff-1.24.1`; sorgenti/build rimangono nel container.

`groff/render.sh` registra stderr anche se la generazione termina con successo:
**un PDF prodotto con exit 0 puo' avere caratteri scomparsi**. Font mancanti e
overflow rimangono visibili nei log e negli output. Nessun preprocessore bidi,
HarfBuzz o riparazione personalizzata dei glifi e' stato aggiunto.

## 4. Implementazione Printpdf

Crate standalone con `[workspace]` proprio: nessun collegamento al workspace
Carta. Il programma usa l'API HTML/CSS effettiva di 0.12.8:
`add_xml_to_document`, `XmlRenderOptions`, `PageMargins`, `build_font_pool`.
Carica direttamente i byte TTF nell'API documentata, tramite `include_bytes!`.
HTML semantico e `classic.css` fisso; geometria A4 e margini tramite API perche'
il supporto CSS `@page` e' incompleto. Nessun paginator alternativo.

Tutti i font e il CSS sono nell'eseguibile. Il font pool esclude la selezione
di font di sistema con filtro vuoto. **Non esclude la scansione del sistema**:
strace osserva 122 file font di sistema aperti e lettura di fontconfig,
compresa configurazione nella home condivisa. I font effettivamente incorporati
nei PDF sono comunque soltanto quelli forniti nel benchmark.

Cargo risolve 132 package inclusa l'applicazione, 131 dipendenze, 223 archi
(compresi target-specific). Feature: `default`, `html`, `text_layout`,
`text_layout_hyphenation`; nessuna feature immagini, SVG o multithreading HTML.
Dipendenze principali del motore: `azul-core/css/layout 0.0.16`,
`allsorts-azul 0.17.2`, `rust-fontconfig 5.0.1`, `lopdf 0.44.0`.
L'albero completo/sintetico e' conservato in `printpdf/logs/`.
Questo e' costo di build e manutenzione, non 131 pacchetti da installare
sul computer dell'utente.

## 5. Font

| File TTF | Versione | Byte |
| --- | --- | ---: |
| SourceSerif4-Regular.ttf | 4.005 | 261.868 |
| SourceSerif4-Italic.ttf | 4.005 | 187.808 |
| SourceSerif4-Semibold.ttf | 4.005 | 272.088 |
| SourceCodePro-Regular.ttf | 2.042 | 210.312 |
| NotoSans-Regular.ttf | 2.008 | 569.208 |
| NotoSansHebrew-Regular.ttf | 3.000 | 26.900 |
| NotoSansArabic-Regular.ttf | 2.009 | 240.456 |
| NotoSansDevanagari-Regular.ttf | 2.002 | 219.212 |
| NotoSansJP-Regular.ttf | 2.004 | 5.767.260 |
| **Totale** | | **7.755.112** |

Quattro font occidentali: 932.076 byte. Source Serif copre anche il greco e il
cirillico del corpus, ma non ebraico, arabo, Devanagari o giapponese.
Noto Sans viene usato per greco/cirillico nella prova di selezione esplicita.
I font Noto per gli altri script sono registrati/montati fin dall'inizio.

Font Adobe da release ufficiali `4.005R` e
`2.042R-u/1.062R-i/1.026R-vf`; Noto da commit fissati dei repository ufficiali
notofonts. Otto file sono statici upstream invariati. Il giapponese e'
istanziato dal TTF variable ufficiale Noto CJK Sans 2.004 a `wght=400` (non il
default upstream 100). Tutti con licenza SIL OFL 1.1, embedding non ristretto.
Versioni interne, URL esatti, commit, SHA-256 e copie delle quattro licenze
sono in `fonts/README.md`, `fonts/manifest.json` e `fonts/*OFL*`.

### Conversione Ufficiale Groff

Usato `install-font.bash` upstream, con FontForge; solo il percorso scratch
e' stato spostato in `/opt`. Per ogni TTF: generazione `.pfa` Type 1,
`.t42` Type 42 e AFM; `afmtodit` genera le metriche groff; l'AFM viene eliminato.
Metriche e Type 42 in `site-font/devps`; Type 1 e symlink metriche in `devpdf`;
registri `download` aggiornati. Nessuna tabella di shaping OpenType originale
rimane utilizzabile dal compositor Type 1.

Risultato di una conversione: 9 `.pfa`, 9 `.t42`, 9 metriche, 9 symlink,
4 registri/backup, **42.239.781 byte** nella tree font completa. I file AFM e
gli intermedi temporanei sono eliminati; resta uno script FontForge di 89 byte.
Per distribuire soltanto PDF si possono escludere Type 42 e duplicati:
**19 file font/registro, 26.996.406 byte**, piu' gli asset di base del device.
Il sottoinsieme occidentale `.pfa` + metriche e' circa 8,92 MB prima dei registri,
contro 0,93 MB di TTF originali. Il CJK aumenta molto le dimensioni.

Le metriche Source Serif sono particolarmente grandi. Conversione non neutra:
warning STAT ignorato, mapping Unicode duplicati, warning `DESC` di afmtodit.
La procedura ufficiale conserva inoltre un difetto CJK concreto, discusso sotto.
FontForge e afmtodit servono in preparazione, non sul computer dell'utente se
Carta distribuisse asset gia' convertiti.

## 6. Risultati Latin

Entrambi producono tre pagine A4 210 x 297 mm con lato sinistro a circa
90,709 pt = 32 mm. Corpo 11 pt, interlinea osservata circa 14,85 pt,
allineamento sinistro, serif, codice mono, nero, nessun header/logo/data/file.
Le differenze di spaziatura dei blocchi impediscono pagine identiche.

**Groff:** titoli Semibold e grassetto effettivi; corsivo corretto; citazioni e
liste semplici; pagina numerata al centro vicino a 282 mm. Ritmo un po' piu'
arioso, codice a 11 pt. I titoli H2 di transizione vengono spostati all'inizio
delle pagine 2/3 con il contenuto successivo. Aspetto gia' convincente per un
documento occidentale sobrio. Link con etichetta identica all'HTML, ma senza
annotazione cliccabile nel percorso roff minimo.

**Printpdf:** corpo, corsivo, codice mono a 9 pt, citazioni e liste funzionano.
Link nero sottolineato. I titoli hanno dimensioni corrette ma **non il peso
richiesto**: `font-weight:600` seleziona Regular, non il Semibold registrato.
La numerazione richiesta con `show_page_numbers:true` **non appare**.
"Il ritmo della pagina" rimane solo a fondo pagina 1 e "Continuita' e
leggibilita'" solo a fondo pagina 2: il paragrafo segue sulla pagina successiva.
Il motore base compone testo leggibile, ma il profilo completo non e' ancora
abbastanza rifinito per considerarlo pronto.

La sequenza patologicamente lunga **deborda in entrambi**, arrivando oltre
il bordo della pagina; groff emette `cannot break line; overset by ...`.
Non sono state aggiunte riparazioni custom. I casi di vedove/orfane sono
osservabili nelle anteprime multipagina, ma non viene certificata una politica
globale corretta: entrambi richiedono ulteriore verifica e un profilo di break.

## 7. Risultati Unicode

| Caso | Groff baseline / con font | Printpdf baseline automatico |
| --- | --- | --- |
| Italiano/accenti/euro | Visivamente buono; apostrofo ASCII diventa tipografico | Buono, estrazione frase esatta |
| Ceco, polacco, romeno | Buoni nel campione, estrazione esatta | Buoni, estrazione esatta |
| Greco | Visibile; Source Serif estrae micro sign al posto di mu | Visibile ed estrazione esatta |
| Cirillico | Visibile ed estrazione esatta | Visibile ed estrazione esatta |
| U+00E9 vs e+U+0301 | Visivamente ragionevolmente equivalenti; sequenze entrambe estraibili | Visivamente equivalenti; sequenze entrambe estraibili |
| Ebraico | Assente baseline; con Noto ordine errato | Visibile; ordine parole/caratteri corretto nel campione |
| Arabo | Assente baseline; con Noto lettere isolate/ordine errato | Joining contestuale e ordine corretti nei campioni |
| Devanagari | Assente baseline; con Noto glifi, ma non shaping completo | Consonanti/vocali composte coerenti col riferimento nel campione |
| Giapponese | Assente baseline; con Noto copertura parziale, perde `日` | Copertura completa nei due campioni |

L'esito riguarda **questi campioni**, non tutte le lingue, i segni diacritici
o tutte le sequenze possibili. Le condizioni exact/pass sono conservate in
`extraction-checks.json`: non si nascondono i falsi e non si confonde la presenza
dei codepoint con la composizione corretta.

## 8. Font Fallback

Paragrafo: `Italiano 日本語 Ελληνικά العربية हिन्दी`.

**Groff baseline:** nessun fallback automatico per i font montati ordinariamente;
scompaiono CJK/arabo/Devanagari. **Fallback nativo configurato:** basta
`.fspecial CR NS NH NA ND NJ` per recuperare molti glifi; non occorre un enorme
sistema custom per la sola copertura. La prova e' `groff-unicode-fallback.pdf`.
La selezione esplicita per span produce un risultato analogo.
Ma non risolve shaping, bidi o mapping sbagliati di glifi gia' disponibili.
Generalizzare richiederebbe font convertiti/metriche affidabili, ordine della
fallback list, gestione delle varianti e un'altra soluzione per script complessi.

**Printpdf:** il body specifica solo Source Serif; l'engine sceglie Noto dai
font registrati quando serve. Nessuna segmentazione manuale nel baseline.
La prova esplicita per span funziona anch'essa, senza migliorare il difetto
di estrazione dello spazio arabo/Hindi. Questo e' sostanzialmente meno codice
di integrazione. Per una distribuzione deterministica rimane da eliminare o
controllare lo scan di fontconfig e da verificare le varianti fallback.

## 9. RTL E Shaping

Confronto visivo dei PNG effettivi con `preview/shaping-reference-raqm.png`:
stessi TTF, Pillow/RAQM 0.10.2 con HarfBuzz 10.2.0 e FriBidi 1.0.16.
Il riferimento e' **solo raster**, non sostituisce alcun PDF rotto.

Groff con Noto mostra ebraico/arabo nell'ordine visuale sinistra-destra
corrispondente ai codepoint logici: il risultato non e' un paragrafo RTL.
L'arabo non applica joining contestuale. Il Devanagari viene disegnato come
sequenza di glifi senza lo stesso riordinamento/congiunzione del riferimento;
la sua estrazione corretta non rende corretto lo shaping.

Printpdf riproduce nel campione ebraico `שלום עולם` e arabo `مرحبا بالعالم`
l'ordine visuale del riferimento, con lettere arabe unite dove previsto.
`printpdf-rtl.pdf` imposta `dir="rtl"`, `direction:rtl`, allineamento destro
e font espliciti: i paragrafi si allineano a destra e l'ordine delle parole
resta corretto. La sequenza con numeri `العربية لغة جميلة 123` mostra 123
al lato previsto, ma **lo spazio prima del numero si perde nell'estrazione**.
Nel baseline senza `dir`, gli span RTL sono composti internamente RTL mentre
il blocco rimane allineato a sinistra: non e' prova di auto-detection della
direzione del paragrafo.

Con Devanagari, sia `नमस्ते दुनिया` sia `हिन्दी` hanno forma compatibile
col riferimento visuale. Combining e legature Latin sono coerenti nei campioni;
`pdftotext` mantiene U+00E9 e la sequenza U+0065 U+0301 senza forzare NFC.
Non e' stata eseguita una suite completa di shaping/conformance UAX #9.

## 10. Pdffonts E Pdftotext

Eseguiti `pdfinfo`, `pdffonts`, `pdftotext -layout`, `pdftotext -raw`,
`qpdf --check` sui quattro PDF principali e sulle prove comuni di scala.
I quattro PDF principali passano qpdf; tutti i font realmente usati risultano
incorporati. Nessun font sostitutivo inatteso nei PDF del corpus comune.
Questo **non** certifica correttezza di contenuto, PDF/A, PDF/X o accessibilita'.

**Groff:** Type 1 custom encoding, `emb=yes`, `sub=yes`, `uni=no`.
Assenza di ToUnicode esplicito: estrazione basata sui nomi dei glifi.
In Source Serif `Καλημέρα κόσμε` torna come `Καληµέρα κόσµε`, con U+00B5
al posto di U+03BC. Selezionare esplicitamente Noto Sans corregge quel campione;
il fallback non interviene perche' Source Serif ha gia' il glifo.
L'apostrofo ASCII del corpus viene sostituito con U+2019.
Ebraico/arabo estratti con sequenze invertite nei documenti con Noto, oltre
all'errore visivo. Per `日本語` il risultato e' `本語`.

**Printpdf:** CID TrueType Identity-H, `emb=yes`, `uni=yes`, `sub=no` in Poppler.
Verifica diretta dei FontFile2: **sono realmente subset**, ma i tag sono
malformati rispetto alla convenzione PDF dei sei caratteri maiuscoli.
Nomi come `F18280+...` contengono cifre: Poppler non li riconosce come subset.
Fonte e producer controllati; non si deduce il subsetting dalla sola dimensione.

| Stream printpdf | Originale byte/glifi | Subset decodificato byte/glifi |
| --- | ---: | ---: |
| Source Serif Regular, Latin | 261.868 / 1.464 | 12.660 / 98 |
| Source Code Pro, Latin | 210.312 / 1.568 | 14.204 / 37 |
| Noto Sans JP, Unicode | 5.767.260 / 17.936 | 4.676 / 11 |
| Noto Sans Arabic, Unicode | 240.456 / 1.661 | 8.492 / 18 |

Printpdf estrae correttamente tutti i campioni per script separati, ma il
paragrafo misto diventa `Italiano 日本語 Ελληνικά العربيةहिन्दी` dopo rimozione
dei controlli bidi e normalizzazione degli spazi: **manca un separatore**.
Accade sia con fallback automatico sia esplicito. Origine precisa (positioning
o estrazione bidi di Poppler) non isolata. Copia/ricerca non sono quindi
perfettamente fedeli per questo caso. I controlli bidi presenti nell'output
raw sono conservati, non scambiati per perdita di lettere.

## 11. Performance

Solo rendering, **nessuna compilazione Rust** nelle misure. Tre ripetizioni
sequenziali per motore/dimensione con `/usr/bin/time`, cache gia' calda dalle
prove del corpus, nessun altro agente di build in parallelo durante le misure.
Wall, user/system CPU, massimo RSS e byte PDF salvati per ogni ripetizione.

Input semanticamente equivalente: un titolo e lo stesso paragrafo ripetuto
5/50/500 volte, senza page break forzati. Le page count **non** sono uguali:
1/9/82 con groff, 1/10/100 con printpdf. Sono tre ordini di scala, non una
misura normalizzata per pagina identica. Il conteggio delle frasi iniziali
estratte conferma 5/50/500 paragrafi in entrambi, non pagine ottenute perdendo
interi blocchi di testo.

Mediane del run finale, dimensioni in byte; RSS convertito da KiB a MiB:

| Motore | Scala nominale | Pagine reali | Wall s | CPU user/system s | Peak RSS MiB | PDF byte |
| --- | ---: | ---: | ---: | --- | ---: | ---: |
| groff | 1 | 1 | 0,79 | 0,83 / 0,07 | 37,6 | 24.549 |
| groff | 10 | 9 | 0,95 | 1,14 / 0,06 | 37,7 | 36.266 |
| groff | 100 | 82 | 2,69 | 4,32 / 0,08 | 37,7 | 131.088 |
| printpdf | 1 | 1 | 0,09 | 0,04 / 0,05 | 46,3 | 10.064 |
| printpdf | 10 | 10 | 0,16 | 0,10 / 0,06 | 58,3 | 44.454 |
| printpdf | 100 | 100 | 1,00 | 0,80 / 0,18 | 240,4 | 387.742 |

Printpdf e' circa 9x/6x/2,7x piu' veloce in questi input; non un rapporto
universale. Groff ha memoria quasi costante e output lungo piu' compatto;
printpdf cresce sensibilmente in RSS e byte con la lunghezza del documento.
Il corpus di prestazioni e' Latin: non si estendono questi numeri a 100 pagine CJK.

Per la pipeline groff GNU time misura il massimo RSS osservato dei processi
figli, **non la somma della memoria dei processi simultanei**. CPU user puo'
superare wall per la pipeline. Per printpdf e' sostanzialmente un singolo
processo. Distrobox startup, conversione Markdown, verifiche e PNG sono esclusi.
Senza Inline::C gropdf segnala un costo nella gestione dei font: non installato
per non aggiungere una variante, quindi i tempi valgono per questa configurazione.

## 12. Dimensione Degli Artefatti

| Documento principale | Pagine | Byte |
| --- | ---: | ---: |
| groff-latin.pdf | 3 | 61.196 |
| printpdf-latin.pdf | 3 | 48.582 |
| groff-unicode.pdf | 1 | 37.620 |
| printpdf-unicode.pdf | 1 | 35.005 |
| groff-unicode-explicit.pdf | 1 | 69.469 |
| groff-unicode-fallback.pdf | 1 | 63.723 |
| printpdf-unicode-explicit.pdf | 1 | 40.710 |

Il groff Unicode baseline e' incompleto: i byte non dimostrano efficienza a
parita' di correttezza. Gli hash e gli altri output sono in `pdf-artifacts.json`.

| Componente distribuito | Byte |
| --- | ---: |
| Installazione completa groff senza font custom | 43.940.566 |
| Installazione completa con font custom | 86.180.347 |
| Stima PDF-only groff, quattro programmi + asset di base | 4.863.584 |
| Font groff PDF-only | 26.996.406 |
| Stima groff + font + carta.tmac, 72 file | 31.861.195 |
| Perl interprete osservato, separato | 3.935.376 |
| Moduli Perl/XS osservati, 62 file, separati | 1.193.293 |
| Librerie ELF/loader osservati groff, separati | 6.413.536 |
| Printpdf eseguibile release con nove TTF + CSS | 21.108.648 |
| Printpdf eseguibile release dopo strip | **19.473.272** |

La stima groff non e' un minimo assoluto ne' una chiusura portabile certificata;
esclude directory overhead, conversion tools, Type 42, documentazione e TTF
originali. Il full install esclude i sorgenti co-locati nel prefisso e il build
tree. I 7,76 MB TTF di printpdf sono **gia' inclusi**, non vanno sommati al binario.
Le librerie glibc di sistema non vengono sommate a printpdf come se fossero
un'installazione extra: lo stesso criterio si applica alle librerie groff.

## 13. Complessita' Di Packaging

**Groff:** si puo' distribuire un bundle ridotto, ma e' un piccolo runtime
composto, non un singolo binario. Servono driver groff, preconv, troff,
gropdf/Perl, moduli Perl, librerie di sistema, device/macros e font convertiti.
Compilazione deve fissare percorsi sensati e preservare registri/font search.
La prova di tree copiata passa rendering e qpdf, ma strace mostra che gropdf
legge ancora i due registri `download` dal prefisso originale: **relocazione
completa non dimostrata**. Non e' inevitabilmente fragile, ma richiede lavoro
di packaging e test per ogni piattaforma che Carta non dovrebbe ignorare.
Dettaglio file per file: `groff-packaging.txt`.

**Printpdf:** lo standalone stripped funziona con font/CSS dentro il binario;
non richiede Rust, FontForge o font installati per la selezione. `ldd` mostra
glibc, libm e libgcc_s, non fontconfig/HarfBuzz/FreeType/Cairo dinamici.
Puo' essere parte del binario Carta con risorse incorporate, senza processo
renderer esterno. Questa prova Linux e' glibc 2.41: compatibilita' con baseline
glibc precedente, macOS e build finale Carta **non verificata**.
Lo scan di fontconfig va ancora considerato per isolamento/prevedibilita'.

Printpdf semplifica il runtime e l'interfaccia, ma sposta il rischio nel codice
del motore HTML/CSS e nel suo grafo di build. Non e' privo di dipendenze:
sono prevalentemente linkate nel binario e aggiornate attraverso Cargo.

## 14. Problemi Trovati

1. Groff: RTL/arabo non corretti anche con Noto espliciti o `.fspecial`.
2. Groff: nessuno shaping completo Devanagari nel percorso testato.
3. Groff: conversione Noto JP perde U+65E5 `日`. FontForge assegna lo stesso
   nome `uni2F47` a U+65E5 e U+2F47; metriche NJ conservano solo `u2F47`.
   Questo e' un difetto concreto della conversione usata, non prova che ogni
   possibile font CJK fallisca. Nessuna riparazione custom e' applicata.
4. Groff: no ToUnicode, estrazione mu/micro sign errata con Source Serif.
5. Groff: bundle ridotto non completamente relocato nella prova.
6. Printpdf: peso 600 ignorato per la famiglia registrata; Semibold esplicito
   in una diagnostica separata **funziona**, ma il baseline resta errato.
7. Printpdf: footer nativo non appare. Sorgenti suggeriscono mismatch tra
   item `Text` standalone del footer e bridge che scarta font hash nonzero;
   nessuna cattura runtime display list per provare completamente la causalita'.
8. Printpdf: H2 soli a fondo pagina; regole keep-with-next non garantite qui.
9. Printpdf: spazi persi nell'estrazione arabo/Hindi e arabo/numeri.
10. Printpdf: tag subset non standard, Poppler riporta `sub=no` nonostante subset.
11. Printpdf: il filtro font disabilita selezione, non scan font di sistema.
12. Entrambi: parola patologica deborda; nessun profilo completo vedove/orfane.

I PDF difettosi sono conservati. Nessun fix al codice dell'applicazione,
nessuna sostituzione del renderer, nessun sistema custom di shaping aggiunto.

Durante la preparazione il nostro macro footer groff lasciava una riga sotto
il footer in un punto di break. E' stato corretto nel macro, non aggirato nel
motore: eiezione nella environment footer vuota e riserva per leading/descent.
L'output precedente e' conservato in `groff/latin-before-trap-fix.pdf`.
I dati finali sopra sono **successivi alla correzione**; controllo bounding box
del corpo/numero pagina e prove dense di break sono in `groff/logs/`.

## 15. Confronto Complessivo

| Criterio | Groff 1.24.1 | Printpdf 0.12.8 |
| --- | --- | --- |
| Latin Classic nel profilo minimo | Migliore: pesi/footer/titoli | Leggibile, ma tre difetti visibili di profilo |
| Font custom | Conversioni + metriche + registri | TTF diretti incorporati |
| Unicode semplice | Buono nel sottoinsieme testato, mapping da controllare | Buono nei campioni, ToUnicode |
| Fallback | Configurazione nativa possibile, font convertiti | Automatico tra font registrati |
| RTL/shaping | Inadeguato per questi script | Nettamente migliore nei campioni reali |
| Estrazione | Mapping errati e RTL invertito | Migliore, ma separatori misti difettosi |
| Velocita' misurata | Piu' lento in questa configurazione | Piu' veloce |
| Memoria lunga | Quasi costante | Cresce fino a circa 240 MiB |
| PDF lunghi | Piu' piccoli | Circa 3x byte a scala grande |
| Packaging | Runtime composto e font convertiti | Singolo eseguibile + ABI di sistema |
| Maturita' del compositor | Alta, ma modello inadatto allo shaping moderno | Motore giovane con bug concreti |

## 16. Raccomandazione Per Carta

**GNU groff e' sufficientemente robusto per un editor UTF-8 moderno?** Non come
export generale multilingue nel percorso testato. Accettare UTF-8 non equivale
a supportare bidi, shaping e copia corretta. Per Latin occidentale e
centroeuropeo senza script complessi e' una soluzione credibile e gradevole.
Cirillico semplice funziona; greco richiede attenzione al mapping del font.
Combining semplice testato va bene, ma non garantisce tutte le combinazioni.

**Quanto costa distribuire font groff?** La procedura ufficiale funziona per
molti glifi, ma crea conversioni/metriche/registri e amplia molto gli asset:
circa 27 MB font PDF-only nel set multilingue, oltre runtime Perl/device.
Il difetto `日` dimostra che bisogna anche validare la conversione, non soltanto
automatizzarla. Senza CJK il costo scende, ma non elimina shaping e ToUnicode.

**Printpdf compone gia' bene Carta Classic?** Il corpo e' gia' buono, non il
profilo completo: Semibold, numero di pagina e titoli legati al paragrafo sono
requisiti da chiudere prima dell'export definitivo. Una famiglia Semibold
esplicita e' un workaround piccolo dimostrato; gli altri punti non sono
silenziosamente risolti qui con un renderer custom.

**Gestisce realmente shaping e fallback meglio?** Si', nei PDF ispezionati e
confrontati col riferimento: arabo unito, ordine RTL, Hindi e giapponese reali,
font fallback automatici e ToUnicode. Non e' un pass universale: estrazione
dei separatori misti e scansione font restano difetti concreti.

**Quanto pesano a runtime?** Standalone printpdf stripped con tutti i font
19,47 MB, contro stima groff PDF/font/macros 31,86 MB piu' Perl e moduli;
memoria groff osservata circa 38 MiB max processo, printpdf circa 47-240 MiB
secondo scala. Printpdf non e' sempre piu' leggero: consuma piu' memoria sui
documenti lunghi; groff non e' sempre piu' leggero: il bundle font e' piu' grande.

**Quale introduce meno complessita' permanente?** Printpdf come libreria Rust,
profilo fisso e font incorporati, a condizione di mantenere i workaround piccoli
e risolvere i bug nel motore/bridge senza trasformare Carta in un compositor.
Il grafo Cargo e il giovane engine Azul sono rischio di manutenzione da fissare
con versioni bloccate e corpus di regressione, non un motivo per preferire una
pipeline che non modella correttamente il testo degli utenti.

**Se si scegliesse groff:** dichiarare esplicitamente export limitato agli
script semplici validati; nessuna garanzia arabo/ebraico/Indic, CJK e mapping
font da validare; escludere promessa di Unicode generale e copia perfetta;
distribuire/gestire groff 1.24.1, Perl e font asset controllati. Non fare passare
caratteri scomparsi con exit 0 come esportazione riuscita.

**Se si scegliesse printpdf:** accettare un engine HTML/CSS non equivalente a un
browser e non ancora completamente rifinito: regressioni layout, CSS ignorato,
API e font resolution, bridge PDF, mapping/subset, crescita RSS. Prima della
decisione di integrazione servono risoluzione dei tre difetti Classic, verifica
separatori RTL, isolamento font e regressioni visuali/testuali sui PDF. Anche
compatibilita' con le piattaforme Carta va misurata, non inferita dal solo Linux.

**Consiglio finale:** fra i due, **printpdf e' il candidato migliore per Carta**
perche' la promessa di documenti UTF-8 e' piu' importante della maturita' di un
layout occidentale, e il packaging evita un secondo runtime composto. Non
consiglio di integrare alla cieca 0.12.8: i difetti osservati devono diventare
criteri di accettazione, non debito nascosto. Groff e' una valida opzione solo
se Carta sceglie consapevolmente un export occidentale ristretto.

**La prova termina qui. Nessuna integrazione e' stata avviata.**

## Output E Validazione

Quattro PDF richiesti, percorsi relativi al repository:

- `experiments/pdf-export/results/groff-latin.pdf`
- `experiments/pdf-export/results/printpdf-latin.pdf`
- `experiments/pdf-export/results/groff-unicode.pdf`
- `experiments/pdf-export/results/printpdf-unicode.pdf`

Preview: `experiments/pdf-export/results/preview/`, tutte le tre pagine Latin
di entrambi e le pagine Unicode, fallback, RTL, piu' il riferimento raster.
L'inventario completo e' `results/FILES.txt`.

Validazione effettiva: build release standalone; `cargo fmt --check`,
`cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`
del solo esperimento (0 unit test, pass); syntax shell e Python; controlli PDF
Poppler/qpdf; estrazione esatta con fallimenti registrati; visione delle pagine
Latin/Unicode/RTL e confronto shaping col riferimento. I test dell'applicazione
Carta non sono stati eseguiti: non e' stata modificata l'applicazione.
