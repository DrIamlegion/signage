# Signage — gratis digitale signage voor Windows en Chrome OS

[![Signage watcher smoke](https://github.com/DrIamlegion/signage/actions/workflows/signage-watcher-smoke.yml/badge.svg)](https://github.com/DrIamlegion/signage/actions/workflows/signage-watcher-smoke.yml)

Een kleine, zelf-gehoste signage-app: zet vanaf je laptop of telefoon slides en video's in
een playlist, en elk scherm (Windows-pc, Chromebook, smart-tv, Raspberry Pi) toont die
playlist in de browser. Geen account, geen abonnement, geen reclame — alleen Node.js.

- **Beheer**: één webpagina met schermen, playlists, planning en media.
- **Speler (ontvanger)**: gewone webpagina die fullscreen werkt, installeerbaar is als app
  (PWA) en blijft doorspelen als de zender of het netwerk wegvalt.
- **Presentatie-editor**: de echte **DeckCraft**-editor (open source, native Rust) voor presentaties met thema's, vormen, tabellen, grafieken, overgangen en animaties. Signage publiceert de dia's als schermweergave; de zender en spelers blijven webgebaseerd.
- **Kosten**: €0. Geen dependencies, geen cloud; het draait op je eigen computer.

![DeckCraft bewerkt een presentatie](docs/deckcraft.png)

## Zender en ontvanger

Dezelfde app heeft twee rollen:

- **Zender** — de computer met het beheer: schermen, media, slides en planning. Daar maak je
  alles klaar en van daaruit wordt het verstuurd.
- **Ontvanger** — de computer of Chromebook die alleen toont. De player-link haalt de playlist
  op, bewaart hem lokaal (playlist in localStorage, mediabestanden in IndexedDB) en speelt
  daarna altijd uit die lokale kopie.

Daardoor blijft het werken als één van de twee uitvalt:

| Situatie | Wat er gebeurt |
| --- | --- |
| Zender even offline | Ontvanger speelt door met de lokale kopie; linksonder verschijnt een klein oranje lampje. |
| Ontvanger start opnieuw (ook zonder netwerk) | Ontvanger begint meteen uit de lokale kopie en synchroniseert zodra de zender er weer is. |
| Wijziging op de zender | Ontvanger ziet hem binnen ±20 seconden en haalt de nieuwe bestanden binnen. |
| Zender voorgoed weg | Ontvanger blijft doorspelen met de laatst ontvangen inhoud. |

## Licentie

Signage is gelicenseerd onder de **Apache License 2.0**. De volledige tekst staat in
`LICENSE`; de copyright- en attributieverklaring staat in `NOTICE`.

Signage Studio bevat de **DeckCraft**-editor. DeckCraft is MIT OR Apache-2.0 en wordt
in Signage Studio onder Apache-2.0 gebruikt. De volledige DeckCraft-notice,
attributie en licentieteksten worden meegeleverd in `licenses/` en zijn samengevat
in `THIRD-PARTY-NOTICES.md`. De ArtCraft-merkbestanden zijn geen onderdeel van de
Signage Studio-installatie; hun voorwaarden staan voor de volledigheid in
`licenses/ArtCraft-brand-LICENSE.txt`.

## 1. Starten

Node.js 18 of nieuwer is genoeg.

```bash
cd signage
node server.js
```

Bij de eerste start wordt een wachtwoord gemaakt en opgeslagen in
`signage/data/admin-password.txt`. Dat staat ook in de console. Open daarna:

```
http://localhost:8100/            <- beheer
```

De console toont ook een netwerk-adres (bijv. `http://192.168.1.24:8100`). Gebruik dat adres
op de Chromebook of een andere pc in hetzelfde wifi-netwerk.

Op Windows kun je ook `start.cmd` dubbelklikken: die start de server en opent het beheer.

## 2. Een scherm toevoegen

1. Tab **Schermen** → **+ Nieuw**.
2. Voeg slides toe: kies media bij **+ Media**, of plak een URL (bijv. een gepubliceerde
   Google Slides-link) bij **+ URL**.
3. Stel per slide de duur in seconden in en het weekrooster (zie hieronder).
4. Kopieer de **player-link** en open die op het scherm.

### Presentaties maken in DeckCraft

Signage Studio **bevat** de volledige **DeckCraft**-editor: geen tweede programma,
geen los venster en geen `deckcraft.exe` ernaast. De editor en de renderer zijn
als Rust-bibliotheken in Signage Studio gebouwd (MIT/Apache-2.0). Je opent hem via
het tabblad **DeckCraft**.

1. Maak een nieuw deck via **Bestand → Nieuw** in de DeckCraft-lintband, of open
   een bestaand `.pptx`/`.deckcraft` uit `data/decks/`.
2. Bewerk en bewaar in dezelfde DeckCraft-interface (`Ctrl+S`).
3. **Publiceren** rendert elke dia met de ingebouwde DeckCraft-renderer en zet ze
   als presentatie in Signage. De zender herlaadt `db.json` direct; spelers
   verversen binnen hun poll-interval. Een herstart is niet nodig.

De bron staat in `deckcraft/deckcraft-main` en wordt meegecompileerd in
`Signage Studio.exe`. Publiceren vervangt bij een tweede keer de vorige render van
hetzelfde bestand, inclusief de bijbehorende media.

De signage-weergave is statisch: animaties en overgangen uit DeckCraft worden
niet meegenomen in de player. Voor de redactie is DeckCraft volledig; voor de
uitzending gebruikt Signage de gerenderde dia's.

### Google Presentaties en andere webpagina's

Plak bij **+ URL** de gewone deel-link van een presentatie. De app maakt er automatisch een
embed-link van die start en doorloopt:

`https://docs.google.com/presentation/d/<id>/embed?start=true&loop=true&delayms=0`

Dat werkt voor elke webpagina die in een iframe mag. Let op: URL-slides hebben internet
nodig — ze worden niet lokaal bewaard, anders dan geüploade bestanden en slides uit de
bouwer. Voor een storing-vrije opstelling kun je dus beter een eigen slide of afbeelding
gebruiken.

### Planning per slide

Onder elke slide staat het rooster:

- **Dagen** — klik `ma` … `zo` aan. Niets aangeklikt betekent elke dag.
- **Tijdvak** — bijvoorbeeld `08:00`–`17:30`. Leeg laten is de hele dag. Een tijdvak dat
  over middernacht loopt (`22:00`–`06:00`) werkt ook: de aangeklikte dag is dan de dag
  waarop het tijdvak *begint*.
- **Periode** (optioneel) — vanaf/tot een datum, bijvoorbeeld alleen in december. Dit staat
  naast het weekrooster: beide moeten kloppen.

De chip achter de naam vat het samen, bijvoorbeeld `ma wo vr · 08:00–17:30`. Verandert een
rooster terwijl het scherm speelt, dan pakt de speler dat bij de volgende slide vanzelf op.

## 3. Op het scherm zetten

**Windows** (Chrome of Edge):

```bat
chrome --kiosk "http://192.168.1.24:8100/player?k=JOUW-SLEUTEL"
```

Of dubbelklik `player-kiosk.cmd` en plak de link. Sluit je kiosk af met `Alt+F4`.

**Chromebook**: open de player-link in Chrome. Zet hem op volledig scherm met de
volledig-scherm-toets (de toets met de vierkantjes, of `F11` op externe toetsenborden).
Voor een vaste app: menu ⋮ → *Cast, opslaan en delen* → **Pagina als app installeren**.
De player vraagt zelf om volledig scherm zodra je hem aanraakt.

**Belangrijk op een Chromebook**: laat hem niet in slaap vallen. Instellingen → Apparaat →
Energiebeheer → *Bij inactiviteit scherm uitschakelen* op **nooit**. De player vraagt via de
Wake Lock API om wakker te blijven zolang het tabblad zichtbaar is.

## 4. Toegang vanaf buiten je netwerk

Standaard luistert de server op je hele netwerk, dus iedereen op je wifi kan meebeheren.
Wil je het scherm op een andere locatie, dan zijn er twee gratis routes:

- **Cloudflare Tunnel** (gratis account): `cloudflared tunnel --url http://localhost:8100`
  geeft een publieke https-link zonder routerinstellingen.
- **Eigen port-forward** in de router naar poort 8100, bij voorkeur achter een reverse proxy
  met https.

Beide zetten het beheer op internet: wijzig dan eerst het wachtwoord in **Instellingen**.

## 5. Windows-installers

Er zijn twee zelfstandige Windows-installers:

- **`Signage-Setup.exe`** installeert de webzender (server + beheer + speler). Die is
  zelfstandig: de app zit in de executable, dus je hebt geen zip of Node-pakketten nodig.
  Zelf bouwen kan met `installer\build.cmd`.
- **`Signage-Studio-Setup.exe`** installeert de native **Signage Studio** met de
  ingebouwde DeckCraft-editor. De installer zet de app in
  `%LOCALAPPDATA%\Programs\Signage Studio`, maakt een startmenu- en
  bureaubladsnelkoppeling en registreert zich bij *Apps en onderdelen*.
  Zelf bouwen kan met `native-installer\build.cmd`.

De native installer laat de gedeelde gegevens in `%LOCALAPPDATA%\Signage\data` met rust;
opnieuw installeren of verwijderen wist je playlists, media en decks dus niet. Verwijderen
van de native app gaat via **Start → Signage Studio → Signage Studio verwijderen**,
*Apps en onderdelen* of `Uninstall.exe` in de installatiemap.

Verwijderen van de webzender gaat via **Start → Signage verwijderen**, *Apps en onderdelen* of
`Signage-Setup.exe --uninstall`. De installer zoekt de bestaande installatie op — via een
expliciete `--dir`, het bij de installatie bewaarde pad, de registersleutel of de
standaardmap — en controleert eerst of daar echt `app\server.js` staat. De map waarin de
setup zelf staat (je downloadmap bijvoorbeeld) wordt nooit opgeruimd. Staat de setup
toevallig in de installatiemap, dan blijven die map en het setup-bestand staan en
verdwijnen alleen de app-bestanden.

## Updates

Bij het starten vraagt de server één keer de nieuwste release op bij
`github.com/DrIamlegion/signage`. Is die nieuwer dan de draaiende versie, dan verschijnt in
het beheer een banner met **Download installer** en daaronder de release-opmerkingen zelf,
uitklapbaar en met dezelfde tekst als op GitHub — je hoeft dus niet naar GitHub om te lezen
wat er veranderd is. Onder **Instellingen → Over** staat je versie en kun je zelf
controleren. **Later** verbergt de banner tot de volgende versie. De ontvanger zelf toont
zodra er een nieuwe versie is een kleine melding **Nieuwe versie x.y.z beschikbaar**
rechtsboven; die verdwijnt na twaalf seconden, blokkeert de playlist niet en verschijnt per
versie maar één keer. De zender ververst de release-info elke vijf minuten en de ontvanger
kijkt elke vijftien seconden, dus een nieuwe release verschijnt zonder herstart van de app
of het scherm.

De controle is bewust voorzichtig: geen internet of een geblokkeerde GitHub-API is geen
fout — de app start gewoon door en probeert het later opnieuw (hooguit elke 10 minuten,
anders elke 6 uur). Er wordt niets automatisch gedownload of geïnstalleerd; je kiest zelf
wanneer je de nieuwe setup uitvoert. Voor een eigen mirror of testomgeving kun je de
voedings-URL overschrijven:

```bash
SIGNAGE_UPDATE_FEED="https://mijn-server/release.json" node server.js
```

1. Dubbelklik `Signage-Setup.exe`.
2. Bevestig de installatiemap en eventueel een bureaubladsnelkoppeling.
3. De installer zet de app in `%LOCALAPPDATA%\Programs\Signage`, maakt snelkoppelingen in
   het startmenu en registreert zich bij *Apps en onderdelen*. Beheerderstoegang is niet nodig.
4. Daarna opent het beheer in je browser en **kies je meteen een wachtwoord**.
   Dat wachtwoord wordt bewaard in `%LOCALAPPDATA%\Signage\data\admin-password.txt`,
   zodat je het later kunt terugvinden.

De installer is tegelijk de launcher:

| Opdracht | Wat het doet |
| --- | --- |
| `Signage.exe` | server starten en het beheer openen |
| `Signage.exe --serve --no-browser` | server starten zonder browser (voor autostart) |
| `Signage.exe --stop` | de server stoppen |
| `Signage.exe --uninstall` | verwijderen; vraagt of de gegevens mee mogen |
| `Signage-Setup.exe --silent --desktop --autostart` | stille installatie met snelkoppelingen |
| `Signage-Studio-Setup.exe --silent` | native app stille installatie |
| `Signage-Studio-Setup.exe --no-desktop` | native app zonder bureaubladsnelkoppeling |
| `Signage-Setup.exe --firewall` | firewall-regel voor poort 8100 toevoegen (vraagt beheerdersbevestiging) |
| `Signage-Setup.exe --firewall-remove` | die regel weer verwijderen |
| `Signage-Setup.exe --no-firewall` | installeren zonder firewall-regel |

De installer heeft een eigen icoon — in de exe, op de snelkoppelingen en bij *Apps en
onderdelen* — en vraagt tijdens de installatie of poort 8100 in de firewall mag worden
vrijgegeven voor privé- en domeinnetwerken. Zeg je nee, dan blijven alleen toestellen op
deze computer bereikbaar. De regel is met `--firewall-remove` of bij het verwijderen zo weer
weg.

Gegevens staan buiten de installatiemap (`%LOCALAPPDATA%\Signage\data`), dus opnieuw
installeren of bijwerken laat je playlists en media staan. Zelf bouwen: `installer\build.cmd`
(vereist Rust en Node.js 18+ om te draaien).

## 6. Hoe het werkt

```
signage/
├─ server.js            # HTTP-server, API, opslag, uploads (geen dependencies)
├─ deckcraft/           # bron van de DeckCraft-editor (MIT/Apache-2.0)
├─ dist/                # Signage Studio (met ingebouwde DeckCraft-editor)
├─ public/
│  ├─ admin.html/.css/.js   # beheer-UI
│  ├─ player.html/.css/.js  # speler
│  ├─ sw.js                 # offline cache voor de speler
│  └─ icon.svg
└─ data/                # automatisch aangemaakt
   ├─ decks/            # .pptx-decks die je in DeckCraft bewerkt
   ├─ db.json           # schermen, playlists, media-lijst
   ├─ media/            # geüploade bestanden
   └─ admin-password.txt
```

- De speler haalt elke 20 seconden de playlist op; wijzigingen in het beheer zijn dus binnen
  een halve minuut zichtbaar. Elke 15 seconden stuurt hij een heartbeat, waaruit de status
  **online/offline** in het beheer komt.
- Offline: de laatste playlist wordt lokaal bewaard en afbeeldingen/PDF's worden gecacht.
  Video's spelen alleen als de server bereikbaar is (die worden niet gecacht wegens grootte).
- Back-up: kopieer de map `signage/data` — daarin zit alles.

## 6. Wat het (nog) niet doet

- DeckCraft bewerkt `.pptx` volledig, maar de signage-speler toont de gerenderde
  dia's als afbeeldingen: animaties en overgangen uit DeckCraft komen niet mee.
- Bestaande presentaties uit de oude Signage-editor worden niet automatisch naar
  `.pptx` omgezet.
- Geen lay-outs met meerdere zones (wel: één slide tegelijk, beeldvullend of passend).
- Geen gebruikersbeheer: één wachtwoord voor het beheer.
- Geen proof-of-play-rapportage.

Dat is precies de functieset waar betaalde signage-pakketten per scherm voor rekenen; hier
zijn het bewust weggelaten snufjes.

## 7. Testen

De live-reload rooktest controleert dat de zender `db.json` via de pollingfallback
herlaadt wanneer `fs.watch` uitvalt. Hij gebruikt een tijdelijke database en raakt
`data/` niet aan:

```bash
node --test test/watcher-smoke.test.js
```

De GitHub Actions-workflow **Signage watcher smoke** draait dezelfde test
automatisch op Linux en Windows zodra de zender of de test wijzigt.

## 8. Release maken

De workflow **Signage release** publiceert een release wanneer je een tag
pusht die met `v` begint. De tag moet overeenkomen met de installer-versie.

1. Zet `version` in `installer/Cargo.toml` en de versiegegevens in
   `installer/app.rc` op hetzelfde nummer, bijvoorbeeld `1.3.5`.
2. Commit en push de wijziging naar `main`.
3. Maak een annotated tag en push die:

   ```bash
   git tag -a v1.3.5 -m "Signage 1.3.5"
   git push origin v1.3.5
   ```

4. De workflow bouwt de webinstaller, de native app en de native installer op
   Windows, controleert de artifacts en publiceert daarna de GitHub-release met
   automatisch gegenereerde release-opmerkingen.

De release bevat:

- `Signage-Setup.exe`
- `Signage-Studio.exe`
- `Signage-Studio-Setup.exe`
- `LICENSE`, `NOTICE` en `THIRD-PARTY-NOTICES.md`
- `licenses/` met de volledige DeckCraft- en third-party-licentieteksten
- `verify-checksums.ps1`
- `SHA256SUMS.txt` met de SHA-256-checksums van de binaries, licentiebestanden en het verificatiescript

Controleer de gedownloade bestanden in dezelfde map. Op Linux, in Git Bash of
in WSL gaat dat met:

```bash
sha256sum -c SHA256SUMS.txt
```

Op Windows zonder `sha256sum` kun je de berekende hashes met PowerShell tonen
en met de inhoud van `SHA256SUMS.txt` vergelijken:

```powershell
Get-FileHash .\Signage-Setup.exe, .\Signage-Studio.exe, .\Signage-Studio-Setup.exe -Algorithm SHA256
Get-Content .\SHA256SUMS.txt
```

Of gebruik het meegeleverde `verify-checksums.ps1`, dat ook bij de
release-assets zit. Vanuit de repository:

```powershell
.\verify-checksums.ps1 -Directory .\pad\naar\downloads
```

Staat het script naast de gedownloade bestanden, dan is dit genoeg:

```powershell
.\verify-checksums.ps1
```

Bij `v1.3.5` hoort dus `version = "1.3.5"` in `installer/Cargo.toml`; bij een
andere tag stopt de workflow voordat er iets wordt gepubliceerd. De workflow
gebruikt dezelfde buildstappen als de gewone build-CI. Zorg dat `native/` en
`deckcraft/` in de repository zijn opgenomen, anders stopt de native build.

Een herrun van dezelfde tag vervangt de bestaande release-assets. Maak voor
een nieuwe inhoud liever een nieuwe patchversie en tag dan een bestaande tag
te verplaatsen.
