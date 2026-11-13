# Zwei Alleinstellungsmerkmale von BuildMC Launcher

Kein anderer Minecraft-Launcher bietet beides in Kombination: einen **KI-gestützten
Einrichtungsassistenten**, der einen Fragebogen selbst verfasst und live nachvollziehbar denkt, und
ein **eingebautes Sicherheitscenter**, das jede Datei prüft, die der Launcher jemals ausführt —
nicht nur, was gerade heruntergeladen wurde.

---

## 1. Der Instanz-Assistent

### Das Problem, das er löst

Eine neue Minecraft-Instanz von Hand einzurichten heißt: die richtige Version wählen, einen
Mod-Loader installieren, die passenden Performance-Mods aus tausenden Möglichkeiten heraussuchen,
Abhängigkeiten und Konflikte selbst durchschauen. Bestehende Launcher bieten dafür höchstens
vorgefertigte Modpacks — starr, nicht auf die eigene Beschreibung zugeschnitten.

### Wie er arbeitet

1. **Freie Beschreibung statt Formular.** Der Nutzer schreibt in eigenen Worten, was er vorhat —
   „Ich will mit zwei Freunden ein Technik-Paket spielen, mein Laptop ist schwach, Bildrate geht vor
   Optik." Mehrere Absätze, keine Kategorien zum Ankreuzen.

2. **Der Fragebogen entsteht live, nicht aus einer festen Liste.** Mit hinterlegtem Schlüssel
   schneidet ein Sprachmodell (Mistral) den Standardkatalog auf genau diese Beschreibung zu —
   streicht, was schon beantwortet ist, ergänzt, was fehlt. Mit **Web-Recherche** eingeschaltet darf
   es den Fragebogen sogar frei neu gestalten, informiert durch eine echte Websuche nach aktuellen
   Mod-Empfehlungen für die genannte Version.

3. **Live-Transparenz statt Black Box.** Während das Modell arbeitet, zeigt ein eigenes Panel neben
   dem Fragebogen in Echtzeit:
   - den **Denkprozess** Wort für Wort, während er entsteht (echtes Server-Sent-Events-Streaming,
     kein simulierter Ladebalken),
   - **Websuchen**, sobald sie beginnen — mit der genauen Suchanfrage und, sobald fertig, den
     gefundenen Quellen (Titel, Domain, anklickbar),
   - **Rückfälle**, offen benannt („Budget beim Nachdenken erschöpft — erneuter Versuch ohne
     Denkschritt") statt eines stillen Fehlschlags.

4. **Die eigentliche Mod-Auswahl kommt nie vom Sprachmodell.** Es liefert nur Suchbegriffe und
   Kategorien — jeder vorgeschlagene Mod stammt aus einer echten, laufenden Suche bei Modrinth (und
   optional CurseForge). Das Modell kann sich also nie einen Mod „ausdenken", der nicht existiert.

5. **Garantie gegen eine leere Antwort.** Fünf ineinandergreifende Sicherungsebenen — von einer
   Leitplanke im Modell-Prompt bis zu einer harten, unbedingten Ersatzsuche direkt vor der Anzeige —
   sorgen dafür, dass der Assistent niemals mit „nichts gefunden" endet, egal wie das Sprachmodell
   den Fragebogen zurechtschneidet oder wie der Nutzer antwortet.

6. **Funktioniert vollständig ohne Schlüssel.** Ohne Mistral-Zugang übernimmt ein fest hinterlegter,
   ebenso ausführlicher Fragenkatalog (14 Fragen: Ziel, Spielstil, Weltgenerierung, Gerät,
   Leistungsziel, Optik, Klang, Komfort, Barrierefreiheit, Server, Umfang, plus die drei technisch
   nötigen Fragen) — derselbe Ablauf, nur ohne die freien Formulierungen und Begründungen des
   Modells.

7. **Plan statt Blindkauf.** Am Ende steht kein sofortiger Download, sondern ein Plan mit Begründung
   je Mod, Warnhinweisen und Größenangabe — erst nach Bestätigung wird etwas installiert.

### Warum das einzigartig ist

Andere Launcher bieten entweder starre Modpacks oder gar keine Anleitung. Keiner lässt ein
Sprachmodell den Fragebogen selbst verfassen, zeigt dessen Denkprozess live an und garantiert
gleichzeitig, dass die Suche nie leer ausgeht — mit einer harten, deterministischen Absicherung
gegen genau das, statt sich auf gutes Modellverhalten zu verlassen.

---

## 2. Das Sicherheitscenter

### Das Problem, das es löst

Minecraft-Mods sind ausführbarer Java-Code aus tausenden unabhängigen Quellen. Kein Launcher prüft
sie vor der Ausführung — weder was gerade heruntergeladen wird, noch was auf anderem Weg in den
Mods-Ordner gelangt: manuell hineinkopiert, aus einem importierten fremden Modpack übernommen, über
„Lokale Mod hinzufügen" eingebunden.

### Was es bewusst nicht ist

Kein selbstgebauter Virenscanner, keine Behauptung von Echtzeitschutz für das ganze System. Echte
Antiviren-Software lebt von täglich gepflegten Signaturdatenbanken und jahrelang trainierter
Verhaltenserkennung — das lässt sich nicht in einem Launcher nachbauen, ohne falsche Sicherheit
vorzutäuschen. Stattdessen: eine ehrliche, tatsächlich wirksame Prüfung genau an der Stelle, an der
sie zählt.

### Wie es arbeitet

**Eine einzige Prüfstelle für alles, was der Launcher ausführt** — direkt vor jedem Spielstart wird
jede `.jar`-Datei im Mods-Ordner geprüft, unabhängig davon, wie sie dorthin gelangte:

1. **VirusTotal** — eine passive Zweitmeinung per Hash-Abfrage gegen rund 70 unabhängige
   Antiviren-Engines. Mit dem eigenen, persönlichen (kostenlosen) Schlüssel des Nutzers, niemals
   einem geteilten. Eine Schwelle von mindestens zwei unabhängigen Treffern verhindert, dass ein
   einzelner Fehlalarm einer einzelnen Engine sofort als Bedrohung gilt.

2. **ClamAV, lokal und parallel** — als eigenständiger Prozess aufgerufen, nie als eingebundene
   Bibliothek (wichtig für Closed-Source-Software: ClamAV steht unter GPL, das betrifft nur das
   *Linken*, nicht das *Aufrufen* eines externen Programms). Bevorzugt wird `clamdscan`, das mit dem
   laufenden `clamd`-Dienst spricht, dessen Signaturdatenbank schon im Speicher liegt — Millisekunden
   statt der ein bis zwei Sekunden, die ein kalter `clamscan`-Start braucht. Ist ClamAV nicht
   installiert, wird diese Schicht einfach übersprungen, kein Fehler.

3. **Beide Schichten laufen gleichzeitig, nicht nacheinander** — und über mehrere Dateien einer
   Instanz hinweg ebenfalls parallel, mit einer sauberen Ratenbegrenzung für VirusTotal (4
   Anfragen/Minute in der kostenlosen Stufe), damit eine Instanz mit vielen Mods nicht sofort über
   die Grenze läuft. Fällt eine Schicht aus — kein Schlüssel, kein ClamAV, ein Netzwerkfehler —
   liefert die andere nahtlos ihr Ergebnis weiter, ohne dass die Prüfung insgesamt ausfällt.

4. **Hash-basierter Zwischenspeicher.** Dieselbe Datei in drei Instanzen wird nur einmal geprüft.
   Eine unveränderte Datei löst nie erneut eine Netzabfrage aus — nach dem ersten Start ist die
   Prüfung praktisch verzögerungsfrei.

5. **Warnen statt sperren, standardmäßig.** Ein Fund hält den Start nicht automatisch auf — ein
   einzelner Fehlalarm soll niemals versehentlich das Spiel unspielbar machen. Ein strengerer Modus
   (Start verweigern, bis bewusst übergangen wird) ist optional zuschaltbar.

6. **Kein automatisches Hochladen.** Eine unbekannte Datei (VirusTotal kennt den Hash noch nicht)
   wird nie ungefragt hochgeladen — der Dateiinhalt würde dadurch öffentlich einsehbar. Nur auf
   ausdrücklichen Klick je Datei.

7. **Eigenes Verwaltungscenter.** Ein neuer Menüpunkt zeigt den Status beider Schichten, erlaubt
   manuelles Neu-Prüfen einzelner Instanzen, listet den Verlauf aller je geprüften Dateien und lässt
   einen erkannten Fehlalarm bewusst als Ausnahme freigeben.

### Warum das einzigartig ist

Kein Minecraft-Launcher prüft heute systematisch, was er ausführt — geschweige denn mit zwei
unabhängigen, gleichzeitig laufenden Prüfschichten an einer einzigen, unumgehbaren Stelle vor jedem
Start. Die Kombination aus ehrlicher Einordnung (keine übertriebenen Versprechen), echter technischer
Tiefe (Parallelität, Ratenbegrenzung, Zwischenspeicher, lizenzsaubere Einbindung von GPL-Software in
Closed-Source-Code) und einer durchdachten Nutzerführung (warnen statt sperren, keine versteckte
Automatik) gibt es sonst nirgends in diesem Bereich.

## 0.4.5 / 0.4.6

- **Start über den offiziellen Minecraft-Launcher.** „Spielen“ legt die Instanz als Profil im offiziellen Launcher an
  (Fabric-Version unter `.minecraft/versions/`, Profil in `launcher_profiles.json` mit dem Instanzordner als Spielordner)
  und öffnet ihn. Anmeldung und Spielstart macht der offizielle Launcher; eigene Anwendungs-ID ist nicht mehr nötig.
- **Export für die Modrinth App** (Instanz → „Für Modrinth exportieren“): schreibt ein `.mrpack` in den Downloads-Ordner.
  Mods von Modrinth werden mit Download-Adresse, sha1 und sha512 verwiesen, alle anderen Mods liegen als Datei im Paket;
  dazu `options.txt`, `servers.dat`, Konfiguration, Ressourcen- und Shaderpakete (Welten nicht).
