# Signage Studio (native)

Signage Studio is de native werkruimte voor Signage: **de volledige DeckCraft-editor
zit in dezelfde binary en hetzelfde venster**. Er is geen tweede programma, geen
los DeckCraft-venster en geen `deckcraft.exe` meer nodig. Geschreven in Rust met
egui/eframe; geen browser en geen webview.

De lintband heeft drie tabbladen: **Bestand**, **DeckCraft** en **Schermen**.
De oude, eigen dia-editor is uit de interface verdwenen.

## DeckCraft

De editor is **DeckCraft 0.17.0**, als bibliotheek ingebouwd in
`Signage Studio.exe`. De bron staat in `signage/deckcraft/deckcraft-main`
(MIT OR Apache-2.0) en wordt meegecompileerd; er staat geen DeckCraft-binary
naast de app en er wordt geen tweede proces gestart.

### Werkwijze

1. Kies **DeckCraft** in de lintband. De volledige DeckCraft-interface staat
   direct onder de Signage-tabs, in hetzelfde venster.
2. Maak een nieuw deck via **Bestand → Nieuw**, of open een bestaand
   `.pptx`/`.deckcraft`.
3. Bewerk en bewaar met `Ctrl+S`.
4. Klik rechtsboven op **Publiceren**. De ingebouwde DeckCraft-renderer maakt
   PNG's van de dia's, zet die als media in de database en maakt een presentatie
   met één beeldvullende dia per pagina. De zender herlaadt `db.json` direct;
   spelers verversen binnen hun poll-interval.

Opnieuw publiceren vervangt de vorige render van hetzelfde bestand, inclusief
de bijbehorende media.

### Publiceren zonder venster

```bash
SIGNAGE_DB=/pad/naar/db.json ./target/release/signage-studio.exe --publish-deck data/decks/deck.pptx
```

Dit gebruikt dezelfde code als de knop **Publiceren** en is handig voor scripts
of een CI-controle.

## Database

De app werkt op dezelfde `db.json` als de zender en ontvanger:

1. `SIGNAGE_DB` (volledig pad naar een db.json), of
2. `%LOCALAPPDATA%\Signage\data\db.json` van de geïnstalleerde app, of
3. `data/db.json` / `../data/db.json` naast de app (ontwikkeldata).

Via **Bestand → Database** kies je een ander bestand. Opslaan gebeurt met
**Bestand → Opslaan** of `Ctrl+S`; de app schrijft atomair (`db.json.tmp` + rename).
De geïnstalleerde zender 1.3.5 of nieuwer volgt `db.json` en herlaadt direct.

## Schermen en playlists

De tab **Schermen** werkt op dezelfde database als de zender. Kies links een scherm
en stel rechts de playlist samen. Het veld **Serveradres** bepaalt de player-link
(standaard `http://localhost:8100`, of de waarde van `SIGNAGE_URL`). Kopieer die link
naar de ontvanger op Chrome OS of een andere pc.

Een gepubliceerde DeckCraft-presentatie verschijnt in de presentatielijst van de
playlist-editor. Daar voeg je hem toe met een duur, weekdagen en tijdsvenster.

## Licentie

Signage Studio is gelicenseerd onder de **Apache License 2.0** (zie `LICENSE` en
`NOTICE`). DeckCraft is MIT OR Apache-2.0 en wordt hieronder onder Apache-2.0
meegecompileerd. De volledige DeckCraft-notice, attributie en licentieteksten
staan in `licenses/` en `THIRD-PARTY-NOTICES.md`.

## Bouwen

```bash
cd native
cargo build --release
cp target/release/signage-studio.exe ../dist/"Signage Studio.exe"
```

De build compileert de DeckCraft-bron uit `signage/deckcraft/deckcraft-main`
mee. Er is geen aparte DeckCraft-installatie of `dist/deckcraft`-map nodig.

Een Windows-installer maken (inclusief snelkoppelingen en uninstaller):

```bat
cd native-installer
build.cmd
```

## Wat nog niet kan

- **Weergave is statisch.** DeckCraft bewerkt het deck volledig, maar Signage
  speelt de gerenderde dia's als afbeeldingen. Animaties, overgangen en
  media-opname uit DeckCraft komen niet mee in de signage-weergave.
- **Geen round-trip van Signage naar DeckCraft.** Bestaande presentaties uit de
  oude editor worden niet automatisch naar `.pptx` omgezet.
- **Nieuwe mediabestanden uploaden** kan nog niet native; bestaande media voeg je
  wel toe aan playlists.
- **Absolute begin-/einddatum** van een playlist-item bewerken kan nog niet; die
  blijft behouden.
