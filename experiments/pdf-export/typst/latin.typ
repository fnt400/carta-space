#import "carta-classic.typ": carta-classic
#show: carta-classic

#heading(level: 1)[#text("Carta Classic: una superficie per scrivere")]

#text("Perché l’estate è già finita — «Davvero?» Costa 10 €. La scrittura è un’attività quotidiana: richiede attenzione, continuità e una pagina tranquilla. Questo documento confronta due compositori, senza chiedere a chi scrive di scegliere il carattere o classificare le proprie idee. Le lettere à, è, é, ì, ò, ù devono restare leggibili, selezionabili e ricercabili. L’apostrofo tipografico e le virgolette italiane non sono decorazioni sostituibili con caselle vuote.")

#heading(level: 2)[#text("Una forma semplice")]

#text("Il testo contiene ")#emph[#text("corsivo ordinario")]#text(" e ")#strong[#text("grassetto ordinario")]#text(", una piccola citazione e liste essenziali. Un ")#link("https://www.gnu.org/software/groff/")[#text("collegamento alla documentazione")]#text(" resta parte del contenuto; non deve diventare un titolo o un elemento colorato. Il codice inline ")#raw("document.save()")#text(" usa un carattere monospaziato. La pagina non porta loghi, nomi di file o date di esportazione: la sola informazione accessoria è un numero piccolo, centrato in basso.")

#quote[#text("Una buona interfaccia lascia spazio al contenuto. Il documento deve rimanere riconoscibile anche quando cambia lo strumento che lo ha composto, e le parole devono sopravvivere alla selezione e alla copia.")]

#list([#text("Un primo punto, senza ornamenti.")], [#text("Un secondo punto abbastanza lungo da controllare il rientro delle righe successive e la continuità del ritmo verticale quando la lista occupa più di una linea.")], [#text("Un terzo punto con lettere accentate: città, qualità, più.")])

#enum([#text("Scrivere il contenuto.")], [#text("Continuare il pensiero senza classificazioni premature.")], [#text("Pubblicare solo quando serve.")])

#heading(level: 3)[#text("Un frammento di codice")]

#raw("fn main() {\n    let prezzo = 10;\n    println!(\"Carta: {} euro\", prezzo);\n}", block: true)

#heading(level: 2)[#text("Il ritmo della pagina")]

#text("Un paragrafo lungo permette di osservare la composizione delle righe. La misura è volutamente contenuta, i margini sono ampi e l’allineamento resta a sinistra. Non vogliamo spazi elastici per raggiungere un bordo destro perfettamente rettilineo. Vogliamo invece una lettura regolare, con righe che finiscono dove il pensiero e le parole lo consentono. Una sillabazione eccessiva sarebbe fastidiosa; l’assenza completa di gestione delle parole lunghe potrebbe però produrre righe debordanti. Entrambi i comportamenti devono essere osservati, non nascosti.")

#text("Nel lavoro quotidiano le pagine non terminano sempre nel punto più conveniente. Un titolo può arrivare molto vicino al margine inferiore, un paragrafo può lasciare una sola riga nella pagina successiva, e una citazione può interrompersi. Questo corpus non impone interruzioni artificiali: lascia ai motori il compito di scegliere i punti di separazione. La prova serve a verificare quanto il profilo minimo sia già utilizzabile senza aggiungere una lunga serie di eccezioni nel codice dell’applicazione.")

#text("La parola pneumonoultramicroscopicsilicovolcanoconiosis è lunga ma reale; la sequenza CartaSupercalifragilistichespiralidosoSenzaPuntiDiInterruzioneABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 mette invece alla prova una situazione patologica. Nessun motore deve fingere che questa situazione non esista. Se il testo esce dal margine, viene spezzato arbitrariamente o perde caratteri, il risultato va conservato e il difetto annotato. Il caso breve «fine.» permette anche di osservare se la composizione crea una riga isolata.")

#heading(level: 3)[#text("Un titolo verso il cambio pagina")]

#text("Il documento continua con lo stesso stile, senza trasformare ogni nuovo argomento in una nuova pagina. L’autore deve poter usare titoli brevi, paragrafi lunghi e parole diverse senza conoscere la macchina di impaginazione. Il carattere principale porta il peso della lettura, mentre il monospaziato resta confinato al codice. La distinzione tra una famiglia serif e una famiglia sans per gli script non coperti è una questione di fallback, non un pretesto per cambiare arbitrariamente il disegno di tutto il documento.")

#text("Un archivio personale contiene ricordi, appunti, bozze e lavori più estesi. Alcuni documenti nascono da una frase sola, altri crescono per settimane. Esportarli dovrebbe essere un’operazione prevedibile, che non costringe a installare un ambiente editoriale completo. La leggerezza del motore conta, ma conta anche la leggerezza dell’integrazione: un piccolo programma esterno può portare molti file e molte ipotesi sul sistema, mentre una libreria più grande può ridurre le dipendenze necessarie sul computer di chi usa Carta.")

#heading(level: 2)[#text("Continuità e leggibilità")]

#text("La prova guarda anche alla possibilità di recuperare le parole dal PDF. Il fatto che il documento appaia corretto non garantisce che il testo sia ricercabile. Le legature come fi, fl, ffi e ffl possono diventare un unico glifo: ufficio, affitto, difficile, flessibile. Il compositore dovrebbe mantenere un mapping ragionevole verso le lettere originali. Lo stesso vale per le lettere accentate e i segni di punteggiatura, che non devono dipendere dall’encoding locale della macchina usata per generare il documento.")

#text("Ogni scelta permanente ha un costo. Distribuire una collezione di font è ragionevole se serve a ottenere un risultato riproducibile; distribuire convertitori, tabelle generate, interpreti e percorsi assoluti richiede una valutazione diversa. Non confrontiamo soltanto la quantità di funzionalità: confrontiamo la robustezza dell’insieme. La compilazione iniziale può essere costosa e il rendering leggero, oppure accadere il contrario. Per questo tempo di compilazione e tempo di generazione vengono misurati separatamente.")

#heading(level: 3)[#text("Un secondo titolo vicino al limite")]

#text("Il lettore non dovrebbe accorgersi di tutti questi dettagli. Dovrebbe vedere una pagina quieta, con una dimensione dei caratteri adeguata e un ritmo verticale coerente. I titoli dovrebbero accompagnare il paragrafo seguente, senza restare soli a fondo pagina. Le citazioni dovrebbero distinguersi con un semplice rientro. Le liste dovrebbero mantenere un ordine comprensibile anche dopo l’estrazione del testo. Nessuno di questi obiettivi richiede un sistema di pubblicazione sofisticato, ma tutti richiedono attenzione ai risultati reali.")

#text("Il confronto termina senza una decisione anticipata. Un motore storico può offrire stabilità e piccoli consumi, ma non necessariamente una gestione moderna delle lingue. Un motore giovane può incorporare shaping e layout nello stesso binario, ma nascondere difetti nelle interruzioni di pagina o nelle proprietà CSS. La scelta per Carta deve nascere da questi documenti concreti, dalle anteprime e dai dati misurati, non da un elenco di promesse o dalla familiarità con un nome.")
