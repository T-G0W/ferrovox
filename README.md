<p align="center"><img src="assets/wordmark.png" alt="Ferrovox – Soundboard by T-GOW" width="600"></p>

# Ferrovox – Soundboard by T-GOW

**Ferrovox** = *ferrum* (Eisen → Rust) + *vox* (Stimme). Ein Soundboard für Windows, geschrieben in Rust,
das Sounds gleichzeitig auf deinen Lautsprechern und in deinem Mikrofon abspielt – mit globalen Hotkeys,
die auch in Spielen funktionieren.

*English: Ferrovox is a Windows soundboard written in Rust. It plays sounds on your speakers and into your
microphone at the same time, with global hotkeys that work in games. The interface follows your Windows
language (German → German, everything else → English).*

Ferrovox ist eine **eigenständige Neuentwicklung**. Es enthält keinen Code, keine Grafiken und keine Sounds
von Soundpad oder anderen Programmen. Übernommen sind nur offene Schnittstellen: Soundpad-Listen (`.spl`)
lassen sich importieren, und die Fernsteuerungsbefehle sind kompatibel, damit vorhandene Stream-Deck-Plugins
weiterlaufen. *Soundpad ist eine Marke ihres Inhabers; Ferrovox steht in keiner Verbindung dazu.*

## Installation

1. **`Ferrovox-Setup-<version>.exe`** von der [Release-Seite](https://github.com/T-G0W/ferrovox/releases/latest) laden und starten – ein Setup für 32- und
   64-Bit-Windows (Windows 10/11), ohne Administratorrechte.
2. Sounds oder **Videos** per Drag & Drop ins Fenster ziehen (MP3, WAV, FLAC, OGG, M4A/AAC, ALAC, AIFF, MP4, MOV, MKV …).
3. Damit andere dich hören: oben rechts auf **„Mikrofon einrichten“ → „Jetzt einrichten – 1 Klick“**.
   Ferrovox lädt das offizielle VB-CABLE direkt von vb-audio.com, prüft die digitale Signatur und installiert es
   still – Windows fragt nur einmal nach Administratorrechten. Danach optional
   **„Als Windows-Standardmikrofon setzen“**: Discord, Spiele & Co. (Mikrofon „Standard“) hören dich und deine
   Sounds ohne weitere Einstellungen. Dein echtes Mikrofon wird automatisch mit durchgeleitet.
4. Hotkey: in der Soundliste auf das Feld in der Spalte **Hotkey** klicken und die Taste drücken – fertig.

Bestehende Soundpad-Liste? Beim ersten Start (leere Liste) gibt es den Button **„Vorhandene Soundpad-Liste übernehmen“**,
oder **Datei → Soundpad-Liste importieren** (inkl. Abspielzähler aus `stats.spl` und Hotkeys).

Windows SmartScreen zeigt beim ersten Start des Setups eventuell „Unbekannter Herausgeber“, weil das Setup
nicht mit einem (kostenpflichtigen) Code-Signing-Zertifikat signiert ist: **Weitere Informationen → Trotzdem ausführen**.

## Automatische Updates

Ferrovox sucht kurz nach dem Start und danach alle 6 Stunden nach einer neuen Version, lädt sie im Hintergrund
und installiert sie still beim Beenden (oder sofort per **„Jetzt neu starten und aktualisieren“**).
Einstellbar unter **Einstellungen → System → Updates**, manuell über **Hilfe → Nach Updates suchen**.

Jedes Update ist mit einem Ed25519-Schlüssel signiert; Ferrovox prüft Signatur, Größe und SHA-256, bevor es
etwas ausführt. Manipulierte Dateien werden verworfen.

## Funktionen

| Bereich | Ferrovox |
|---|---|
| Wiedergabe Lautsprecher / Mikrofon / beides | ✔ + Modus pro Sound |
| Mehrere Sounds gleichzeitig | ✔ „Sounds überlagern“ |
| Mikrofon-Einspeisung | virtuelles Mikrofon per **Ein-Klick-Installation aus der App**, ohne Abschalten von Windows-Schutzfunktionen |
| Für Discord automatisch | ✔ optional als Windows-Standardmikrofon, mit Rückkopplungsschutz |
| Hotkeys pro Sound, ~45 Spezial-Hotkeys | ✔ inkl. **Maustasten 4/5/Mitte** |
| Modifier+Index, Modifier+Numcode, Kategorie-Index, L/R-Modifier, Hotkeys durchreichen | ✔ |
| Auto-Keys (Push-to-Talk automatisch) mit Bedingungen | ✔ Scancode-basiert (spieltauglich) |
| Hotbar mit Seiten, Farben, Drag & Drop | ✔ Rasterlayout frei wählbar |
| Kategorien, Index, Zufalls-Hotkey, verstecken | ✔ |
| Lautstärke-Normalisierung + Sprachmessung | ✔ (gegated RMS, Clipping-Schutz) |
| Eigene Lautstärke pro Sound, nicht-destruktives Zuschneiden | ✔ |
| Soundeditor (Schneiden, Fades, Rückwärts, Boost …) | ✔ |
| Recorder (Lautsprecher-Loopback / Mikrofon) | ✔ (WAV) |
| Text-to-Speech (Windows-Stimmen) | ✔ + als Sound speichern |
| Andere Apps leiser (Ducking) | ✔ direkt pro Sitzung, Stärke einstellbar |
| Sprachaktivierung auslösen, PTT-/Mikro-Verzögerung | ✔ |
| Benachrichtigungstöne | ✔ erzeugte Töne (keine Fremd-Assets) |
| Fernsteuerung | ✔ eigene Pipe + Soundpad-kompatible Pipe + `-rc` |
| Vorladen von Hotkey-Sounds (null Verzögerung) | ✔ LRU-Cache |
| Undo/Redo, tote Einträge, Mischen, Export, Statistik | ✔ |
| Sprachen | Deutsch + Englisch, automatisch nach Windows-Sprache |
| Setup + automatische Updates | ✔ 32 + 64 Bit, signiert |

## Oberfläche

Schlichtes Soundboard-Layout im dunklen Studio-Look: flache Panels mit Kopfzeile, blaue Akzente und Werte,
Linien-Icons (Segoe Fluent Icons), dunkle Titelleiste.

- **Oben:** Menü, Werkzeugleiste (Wiedergabe, Modus, Lautstärke, Hotkeys/Auto-Keys, Sprache) und „Jetzt läuft“ mit Fortschritt und Pegeln
- **Links:** Kategorien · **Mitte:** Soundliste (Spalten über *Ansicht → Spalten*) · **Unten:** Hotbar
- Themes: Studio (Blau), Studio (Kupfer), Hell

## Fernsteuerung

Ferrovox lauscht auf `\\.\pipe\ferrovox` und – solange Soundpad nicht läuft – auch auf
`\\.\pipe\sp_remote_control`, damit vorhandene Stream-Deck-Plugins und Skripte weiterlaufen.

```bash
ferrovox.exe -rc "DoPlaySound(3)"
```

Wichtige Befehle: `DoPlaySound(i[, lautsprecher, mikro])`, `DoPlaySoundFromCategory(k, i)`,
`DoPlayRandomSound()`, `DoPlayRandomSoundFromCategory(k)`, `DoStopSound()`, `DoTogglePause()`,
`DoJumpMs(ms)`, `DoSeekMs(ms)`, `SetVolume(0-100)`, `DoToggleMute()`, `DoAddSound(pfad)`,
`DoStartRecordingSpeakers()`, `DoStartRecordingMicrophone()`, `DoStopRecording()`, `DoSearch(text)`,
`DoSelectIndex(i)`, `DoUndo()`, `DoRedo()`, `GetSoundlist()`, `GetCategories()`, `GetPlayStatus()`,
`GetPlaybackPositionInMs()`, `GetVolume()`, `IsAlive()`. Ferrovox-Extras: `DoPlaySoundByName(text)`,
`DoTts(text)`, `DoSetPlayMode(speakers|microphone|both)`, `ShowWindow()`, `ShowPanel(settings:hotkeys)`.

Antworten: Werte direkt, sonst `R-200` (ok), `R-204` (kein Inhalt), `R-404` (unbekannt/nicht gefunden).

## Daten

- Soundliste, Einstellungen, Log: `%APPDATA%\T-GOW\Ferrovox` (tägliche Backups in `backups\`)
- Programm: `%LOCALAPPDATA%\Programs\Ferrovox` (Deinstallation über Windows-Einstellungen → Apps)
- Portabel: Umgebungsvariable `FERROVOX_DATA_DIR` auf einen Ordner setzen
- Aufnahmen: `Musik\Ferrovox Aufnahmen` bzw. `Music\Ferrovox Recordings` (einstellbar)

Ferrovox sendet keine Nutzungsdaten. Netzwerkzugriffe gibt es nur für die Update-Prüfung (GitHub) und
die VB-CABLE-Einrichtung (vb-audio.com).

## Selbst bauen

Voraussetzungen: Rust (stable, MSVC, Ziele `x86_64-pc-windows-msvc` und `i686-pc-windows-msvc`),
Windows-SDK und für das Setup [Inno Setup 6](https://jrsoftware.org/isinfo.php).

```bash
cargo build --release
```

```bash
cargo test --release
```

Komplettes Release (beide EXEs, Lizenztexte, Setup, signierte Update-Beschreibung) nach `dist\release\`:

```bash
powershell -ExecutionPolicy Bypass -File release\build-release.ps1 -Repo T-G0W/ferrovox
```

## Neue Version veröffentlichen

1. `version` in `Cargo.toml` erhöhen, `release\notes-de.txt` / `notes-en.txt` anpassen.
2. Commit, Tag `v<version>` setzen und pushen – GitHub Actions (`.github/workflows/release.yml`) baut
   die Setup.exe für 32 + 64 Bit, signiert `latest.json` und veröffentlicht das Release.
3. Alle installierten Ferrovox-Versionen finden das Update automatisch.

Einmalig: Das Secret `FERROVOX_SIGNING_KEY` im Repository anlegen (Inhalt von
`%USERPROFILE%\.ferrovox-release\signing-key.txt`). Den privaten Schlüssel sicher aufbewahren und nie
veröffentlichen – ohne ihn lassen sich keine Updates mehr signieren. Das Repository (oder ein separates
Release-Repository) muss öffentlich sein, damit Ferrovox die Updates ohne Anmeldung laden kann.

## Aufbau

```
src/
  main.rs          Start, Einzelinstanz, -rc, Logging
  core.rs          Wiedergabelogik, Hotkey-Tabelle, Auto-Keys, Aufnahme, TTS, Undo/Redo
  audio/           Engine (cpal/WASAPI), Echtzeit-Mixer, Dekoder (Symphonia), Cache, DSP, Ducking, TTS
  hook.rs          Low-Level-Tastatur-/Maus-Hook für globale Hotkeys
  autokeys.rs      SendInput (Scancodes) für Push-to-Talk
  remote.rs        Named-Pipe-Fernsteuerung
  library.rs       Sounds, Kategorien, Hotbar (JSON)
  spl.rs           Import/Export von .spl-Soundlisten
  settings.rs      Einstellungen + Spezial-Hotkeys
  i18n.rs          Deutsch/Englisch, automatisch nach Windows-Sprache
  update.rs        Auto-Updater (GitHub Releases, Ed25519-Signatur)
  vmic.rs          Ein-Klick-Einrichtung des virtuellen Mikrofons
  ui/              Oberfläche (egui): Liste, Kategorien, Hotbar, Einstellungen, Editor, Werkzeuge
installer/         Inno-Setup-Skript (32 + 64 Bit)
release/           Release-Skript und Versionshinweise
tools/release/     Schlüssel erzeugen, Updates signieren, Lizenztexte sammeln
```

## Bekannte Grenzen

- Ohne Treiber lässt Windows kein Programm in ein Mikrofon schreiben. Ferrovox installiert dafür VB-CABLE
  (Donationware von VB-Audio) per Klick. Je nach System verlangt VB-CABLE einen Neustart, bis das Gerät erscheint.
- Aufnahmen und Editor speichern als WAV (kein MP3/M4A-Encoder).
- Opus-Audio (häufig in WebM) wird vom Dekoder nicht unterstützt.

© 2026 T-GOW
