# Ritardometro

Il timer per talk che ti dice **se sei in anticipo o in ritardo**, con un omino cartoon che fa il tifo per te (o si dispera) davanti al pubblico.

![Gli stati dell'avatar](docs/stati.png)

Resta sempre in primo piano sopra le slide, legge dallo schermo il numero della slide in cui sei e lo confronta con il tempo trascorso:

- **guanto verde in alto**: sei in anticipo, puoi rallentare
- **guanto rosso in alto**: sei in ritardo, accelera
- **faccia**: sorride se sei in linea, si preoccupa se perdi tempo, suda se sei nel panico
- **bandierine**: a metà talk, quando mancano 5 minuti, all'inizio delle domande e allo scadere del tempo

![Le bandierine](docs/bandierine.png)

## Come funziona

L'idea di fondo è **una slide al minuto**: se hai 30 minuti e 30 slide, al minuto 12 dovresti essere più o meno alla slide 13. Le slide separatrici servono a recuperare, e il Ritardometro te lo fa vedere subito.

1. Un thread cattura ogni mezzo secondo l'angolo in basso a destra dello schermo, dove Marp mette la paginazione.
2. L'immagine passa all'**OCR integrato di Windows** (`Windows.Media.Ocr`): niente Tesseract o altro da installare.
3. Il numero letto (`12`, `12 / 40`, `12 di 40`…) viene accettato solo se arriva uguale due volte di fila, così un errore di lettura isolato non fa impazzire l'avatar.
4. Lo scarto è `(slide fatte - slide attese) × secondi per slide`: positivo vuol dire anticipo, negativo ritardo.

Se l'OCR non c'è o è disattivato, funziona come semplice conto alla rovescia con le bandierine.

## Installazione

Scarica `ritardometro.exe` dalla pagina [Releases](https://github.com/matteobaccan/ritardometro/releases) (o dagli artifact dell'ultima build in **Actions**) e mettilo in una cartella a tua scelta.

Oppure compilalo tu (serve [Rust](https://rustup.rs)):

```sh
cargo build --release
# l'eseguibile è in target/release/ritardometro.exe
```

## Uso

```sh
ritardometro.exe                          # usa ritardometro.toml accanto all'exe
ritardometro.exe devsecopsday.toml        # un file di configurazione per ogni talk
ritardometro.exe --minuti 25 --slide 28   # sovrascrive durata e numero di slide al volo
ritardometro.exe --demo                   # simula un talk da 30 minuti in poco più di un minuto
```

Al primo avvio viene creato `ritardometro.toml` con tutti i valori commentati.

Prima di salire sul palco:

1. Apri il PDF delle slide a schermo intero (schermo duplicato).
2. Avvia il Ritardometro: compare in alto a destra, trascinalo dove preferisci.
3. Parte da solo quando passi dalla prima alla seconda slide, oppure con un clic.

### Comandi

| Azione | Come |
|---|---|
| Avvia / pausa | clic sull'overlay, oppure `Spazio` |
| Sposta l'overlay | trascina con il tasto sinistro |
| Menu (correzioni, configurazione, esci) | tasto destro |
| Azzera | `R` |
| Tempo ±1 minuto | `+` / `-` |
| Slide ±1 a mano (senza OCR) | `→` / `←` |
| Esci | `Esc` |

I tasti funzionano quando l'overlay ha il focus (cliccaci sopra); durante il talk il focus resta sul visualizzatore PDF, quindi le frecce continuano a cambiare slide come sempre.

## Configurazione

Le opzioni principali di `ritardometro.toml`:

```toml
durata_minuti = 30        # parte parlata, domande escluse
domande_minuti = 10       # conto alla rovescia delle Q&A a fine talk
totale_slide = 30         # 0 = usa il totale letto dallo schermo ("12 / 40")
avvio_automatico = true   # parte quando passi dalla slide 1 alla 2
tolleranza_secondi = 60   # entro questo scarto sei "in linea"
panico_secondi = 180      # oltre questo ritardo l'avatar suda

[ocr]
# Rettangolo del contatore, in frazioni dello schermo (default: angolo in basso a destra)
x = 0.85
y = 0.88
larghezza = 0.15
altezza = 0.12
salva_debug = false       # true = salva la cattura in ritardometro-debug.png

[[milestone]]
percentuale = 50
testo = "METÀ!"

[[milestone]]
mancano_minuti = 5
testo = "-5 MIN"
```

Puoi aggiungere quante `[[milestone]]` vuoi, con `percentuale` del talk oppure `mancano_minuti`.

### Se l'OCR non legge il numero

- Metti `salva_debug = true`, avvia e guarda `ritardometro-debug.png`: deve contenere il numero della slide e poco altro. Sposta `x`, `y`, `larghezza`, `altezza` finché non lo inquadra bene.
- Non mettere l'overlay sopra il contatore, altrimenti legge se stesso.
- L'OCR usa le lingue installate in Windows. Se compare un errore sull'OCR non disponibile, aggiungi una lingua (ad esempio Italiano o Inglese) da *Impostazioni > Ora e lingua > Lingua* con il componente di riconoscimento ottico.
- In Marp, un contatore come `12 / 40` funziona meglio di un numero isolato, perché è più facile da riconoscere:

  ```css
  section::after { content: attr(data-marpit-pagination) ' / ' attr(data-marpit-pagination-total); }
  ```

## Struttura del codice

| File | Cosa fa |
|---|---|
| `src/main.rs` | argomenti da riga di comando e creazione della finestra |
| `src/app.rs` | overlay: stato del talk, disegno, interazione |
| `src/avatar.rs` | l'omino vettoriale, tutto disegnato con il painter di egui |
| `src/pacing.rs` | logica pura: cronometro, anticipo/ritardo, milestone (con test) |
| `src/ocr/` | cattura dello schermo (`xcap`) e OCR di Windows, parsing del numero |
| `src/config.rs` | `ritardometro.toml`, valori di default e validazione |

Interfaccia con [egui/eframe](https://github.com/emilk/egui) e renderer OpenGL, il più affidabile per le finestre trasparenti su Windows.

```sh
cargo test   # logica di pacing, parsing OCR e configurazione
```

## Licenza

MIT
