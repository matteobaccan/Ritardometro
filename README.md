# Ritardometro

Il timer per talk che ti dice **se sei in anticipo o in ritardo**, con la caricatura animata dello speaker che fa il tifo per te (o si dispera) davanti al pubblico.

![Gli stati dell'avatar](docs/stati.png)

Resta sempre in primo piano sopra le slide, legge dallo schermo il numero della slide in cui sei e lo confronta con il tempo trascorso:

- **aura e bollino verde**: sei in anticipo, puoi rallentare
- **aura e bollino rosso**: sei in ritardo, accelera
- **espressione della caricatura**: sorride e annuisce se sei in linea, aggrotta le sopracciglia e si preoccupa se perdi tempo, sgrana gli occhi e suda in stile fumetto se sei nel panico
- **fumetti milestone**: compaiono a metà talk, quando mancano 5 minuti, all'inizio delle domande (`DOMANDE?`) e allo scadere del tempo (`STOP!`)

![I fumetti delle milestone](docs/bandierine.png)

## Come funziona

L'idea di fondo è **una slide al minuto**: se hai 30 minuti e 30 slide, al minuto 12 dovresti essere più o meno alla slide 13. Le slide separatrici servono a recuperare, e il Ritardometro te lo fa vedere subito.

1. Un thread cattura ogni mezzo secondo l'angolo in basso a destra dello schermo, dove Marp mette la paginazione.
2. L'immagine passa all'**OCR integrato di Windows** (`Windows.Media.Ocr`): niente Tesseract o altro da installare.
3. Il numero letto (`12`, `12 / 40`, `12 di 40`…) viene accettato solo se arriva uguale due volte di fila, così un errore di lettura isolato non fa impazzire l'avatar.
4. Lo scarto è `(slide fatte - slide attese) × secondi per slide`: positivo vuol dire anticipo, negativo ritardo.

Se l'OCR non c'è o è disattivato, funziona come semplice conto alla rovescia con i fumetti delle milestone.

## Installazione

Scarica l'archivio per il tuo sistema dalla pagina [Releases](https://github.com/matteobaccan/Ritardometro/releases/latest) ed estrailo in una cartella a tua scelta:

| Sistema | File |
|---|---|
| Windows 10/11 (x64) | `ritardometro-vX.Y.Z-windows-x64.zip` |
| Linux (x64) | `ritardometro-vX.Y.Z-linux-x64.tar.gz` |
| macOS Apple Silicon | `ritardometro-vX.Y.Z-macos-arm64.tar.gz` |
| macOS Intel | `ritardometro-vX.Y.Z-macos-x64.tar.gz` |

> La lettura automatica della slide (OCR) è disponibile solo su Windows; su Linux e macOS il Ritardometro funziona come timer (cambi slide con ← →).
> Su macOS il binario non è firmato: al primo avvio usa tasto destro → Apri, oppure `xattr -d com.apple.quarantine ./ritardometro`.

La versione installata e il link al repository sono in fondo alla finestra di configurazione (clic destro sull'avatar).

Oppure compilalo tu (serve [Rust](https://rustup.rs)):

```sh
cargo build --release
# l'eseguibile è in target/release/ritardometro(.exe)
```

## Pubblicare una nuova versione

1. Aggiorna `version` in `Cargo.toml` (es. `0.2.0`) e fai commit.
2. Crea e pusha il tag corrispondente:
   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```
3. Il workflow [`release.yml`](.github/workflows/release.yml) verifica che il tag coincida con `Cargo.toml`, compila per Windows, Linux e macOS (Intel + Apple Silicon) e pubblica gli archivi con `SHA256SUMS.txt` nella GitHub Release.

## Uso

```sh
ritardometro.exe                          # usa ritardometro.toml accanto all'exe
ritardometro.exe devsecopsday.toml        # un file di configurazione per ogni talk
ritardometro.exe --minuti 25 --slide 28   # sovrascrive durata e numero di slide al volo
ritardometro.exe --demo                   # simula un talk da 30 minuti in poco più di un minuto
```

| Opzione | Significato |
|---|---|
| `file.toml` | file di configurazione da usare (default: `ritardometro.toml` accanto all'eseguibile) |
| `-m N`, `--minuti N` | durata della parte parlata in minuti (accetta anche decimali, es. `12,5`) |
| `-s N`, `--slide N` | numero totale di slide (`0` = usa il totale letto dallo schermo) |
| `--demo` | modalità dimostrativa: talk simulato a velocità 30x, senza OCR |
| `-h`, `--help` | mostra la sintassi |

Al primo avvio viene creato `ritardometro.toml` con tutti i valori commentati.

Prima di salire sul palco:

1. Apri il PDF delle slide a schermo intero (schermo duplicato).
2. Avvia il Ritardometro: compare nell'angolo scelto (default in alto a destra); trascinalo e ridimensionalo dove preferisci, anche su un altro monitor.
3. Parte da solo quando passi dalla prima alla seconda slide, oppure con un clic.

### Tastiera

I tasti funzionano quando l'overlay ha il focus (cliccaci sopra); durante il talk il focus resta sul visualizzatore PDF, quindi le frecce continuano a cambiare slide come sempre.

| Tasto | Azione |
|---|---|
| `Spazio` | avvia / mette in pausa il timer (in pausa l'OCR smette di leggere lo schermo) |
| `R` | azzera timer e bandierine |
| `+` (o `=`) | tempo trascorso +1 minuto |
| `-` | tempo trascorso −1 minuto |
| `→` o `PagGiù` | slide successiva (correzione manuale, utile senza OCR) |
| `←` o `PagSu` | slide precedente |
| `Ctrl` + `+` | ingrandisce l'overlay |
| `Ctrl` + `-` | rimpicciolisce l'overlay |
| `Ctrl` + `0` | dimensione normale (100%) |
| `Esc` o `Q` | chiude il menu di configurazione se aperto, altrimenti esce dal programma |

### Mouse

| Gesto | Azione |
|---|---|
| clic sinistro sull'overlay | avvia / pausa |
| trascina con il tasto sinistro | sposta l'overlay (anche tra monitor diversi) |
| trascina un bordo o un angolo | ridimensiona; l'angolo opposto resta fermo |
| rotellina | ingrandisce / rimpicciolisce |
| clic destro | apre / chiude la finestra di configurazione |

### Finestra di configurazione (clic destro)

| Comando | Cosa fa |
|---|---|
| Pausa / Avvia, Azzera | come `Spazio` e `R` |
| Tempo −1 / +1 min | corregge il tempo trascorso |
| Slide −1 / +1 | corregge la slide corrente |
| Dimensione − / + / 100% | scala dell'overlay |
| Monitor OCR: Auto | legge il monitor su cui si trova il Ritardometro |
| Schermi trovati: Mon 1, Mon 2… | forza la lettura su un monitor specifico |
| Mostra zona OCR sullo schermo | illumina per un paio di secondi il riquadro letto dall'OCR, con il numero del monitor |
| Frequenza OCR | ogni quanto leggere lo schermo (200 / 350 / 500 ms) |
| Salva ritardometro-debug.png | salva ogni cattura per calibrare la zona OCR |
| Trasparenza: Sfondo, Avatar e testi | opacità del riquadro e dell'intero overlay |
| Apri file di configurazione | apre `ritardometro.toml` nell'editor (le modifiche richiedono un riavvio) |
| Ritardometro vX.Y.Z · GitHub | versione installata e link al repository, da cui scaricare gli aggiornamenti |
| Chiudi menu / Esci dal programma | — |

Le modifiche fatte dal menu valgono per la sessione corrente; per renderle permanenti scrivile in `ritardometro.toml`.


## Configurazione

Tutte le opzioni di `ritardometro.toml` (il file generato al primo avvio le contiene già commentate):

```toml
durata_minuti = 30        # parte parlata, domande escluse
domande_minuti = 10       # conto alla rovescia delle Q&A a fine talk (0 = nessuno)
totale_slide = 30         # 0 = usa il totale letto dallo schermo ("12 / 40")
avvio_automatico = true   # parte quando passi dalla slide 1 alla 2
tolleranza_secondi = 60   # entro questo scarto sei "in linea"
panico_secondi = 180      # oltre questo ritardo l'avatar suda
scala = 1.0               # dimensione iniziale dell'overlay (ridimensionabile anche al volo)
opacita_sfondo = 0.70     # opacità del riquadro dietro all'avatar (0.0 - 1.0)
opacita = 1.0             # opacità di avatar e testi (0.15 - 1.0)
posizione = "alto-destra" # alto-sinistra | alto-destra | basso-sinistra | basso-destra
margine = 24              # distanza dal bordo dello schermo

[ocr]
attivo = true             # false = solo timer
monitor = 0               # 0 = automatico (lo schermo del Ritardometro), 1, 2, 3... = monitor fisso
# Rettangolo del contatore, in frazioni dello schermo (default: angolo in basso a destra)
x = 0.85
y = 0.88
larghezza = 0.15
altezza = 0.12
intervallo_ms = 250       # ogni quanto leggere lo schermo
ingrandimento = 3         # ingrandimento della cattura prima dell'OCR
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

- L'OCR legge sempre un rettangolo **dello schermo** (non della finestra attiva): con `monitor = 0` usa il monitor su cui si trova il Ritardometro. Usa **Mostra zona OCR sullo schermo** dal menu per vedere quale monitor e quale area sta leggendo.
- Attiva **Salva ritardometro-debug.png** dal menu (o `salva_debug = true`) e guarda `ritardometro-debug.png`: deve contenere il numero della slide e poco altro. Sposta `x`, `y`, `larghezza`, `altezza` finché non lo inquadra bene.
- Non mettere l'overlay sopra il contatore, altrimenti legge se stesso.
- L'OCR usa le lingue installate in Windows. Se compare un errore sull'OCR non disponibile, aggiungi una lingua (ad esempio Italiano o Inglese) da *Impostazioni > Ora e lingua > Lingua* con il componente di riconoscimento ottico.
- In Marp, un contatore come `12 / 40` funziona meglio di un numero isolato, perché è più facile da riconoscere:

  ```css
  section::after { content: attr(data-marpit-pagination) ' / ' attr(data-marpit-pagination-total); }
  ```


## Struttura del codice

| File | Cosa fa |
|---|---|
| `src/main.rs` | argomenti da riga di comando e creazione della finestra ridimensionabile |
| `src/app.rs` | overlay: stato del talk, disegno, ridimensionamento e interazione |
| `src/avatar.rs` | caricatura animata tramite mesh 2D deformabile (espressioni, respiro, sudore e fumetti comic) |
| `src/pacing.rs` | logica pura: cronometro, anticipo/ritardo, milestone (con test) |
| `src/ocr/` | cattura dello schermo (`xcap`) e OCR di Windows, parsing del numero |
| `src/config.rs` | `ritardometro.toml`, valori di default e validazione |

Interfaccia con [egui/eframe](https://github.com/emilk/egui) e renderer OpenGL, il più affidabile per le finestre trasparenti su Windows.

```sh
cargo test   # logica di pacing, parsing OCR e configurazione
```

## Licenza

MIT
