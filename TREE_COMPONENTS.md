# Mix-Baum – Bestandteile (zum Beschreiben des Soll-Zustands)

Trage pro Element unter **Soll:** ein, wie es aussehen soll (Größe, Farbe, Glühen,
Form, Schrift, Position). „Ist" = aktueller Stand in `crates/mixtree-gui/src/views/graph.rs`.
Alle Positionen liegen in einem festen logischen Koordinatensystem
(`tree_layout.rs`, viewBox **906 × 701**, Werte = normalisiert × viewBox) und
werden proportional ins Fenster skaliert. Zeichenreihenfolge = Reihenfolge unten.

---

## 0. Koordinatensystem / Gesamtfläche
- Ist: feste viewBox 906×701, zentriert, proportional skaliert; Pan (ziehen) + Zoom (Rad).
- Soll:

## 1. Hintergrund (Canvas)
- Ist: fast schwarzes Rechteck (#06090F), Ecken abgerundet (r≈12).
- Soll:

## 2. Ambientes Themen-Licht (pro Genre-Bereich)
- Ist: weicher farbiger Lichtfleck hinter jedem Genre; 5 Ringe, Radius ~95→ fallend, sehr geringe Deckkraft (α≈3), Genre-Farbe.
- Soll:

## 3. Genre-Äste (Hub → seine Tracks)
- Ist: gebogene Bézier-Linien in Genre-Farbe (α≈130), Kernbreite 1.6, weiches Glühen (3 Lagen), Pfeilspitze am Track-Rand; an den Knotenrändern beschnitten.
- Soll:

## 4. Alternativ-Kanten (vom aktuellen Track)
- Ist: gebogene cyanfarbene Linien (#2D9CFF, α≈170), Breite 2.0, Glühen, Pfeilspitze; nur vom aktuell gewählten Track ausgehend.
- Soll:

## 5. Roter Empfehlungspfad (Plasma)
- Ist: eigene Ebene — breite weiche rote Glow-Schicht + kräftiger roter Kern (pulsiert leicht) + heller wandernder Licht-Komet (10er-Schweif) in Wiedergaberichtung.
- Soll:

## 6. BPM-Übergangs-Pills
- Ist: kleine dunkle, abgerundete Pille mit Text „+6 BPM" (rot am roten Pfad, cyan an Alternativen), am Mittelpunkt der Kante.
- Soll:

## 7. Genre-Knoten
### 7.1 Kreis
- Ist: Kreis Radius ~22, dunkles Zentrum (#0B121E), weicher Genre-farbiger Leuchtring (mehrlagig) + Kernring.
- Soll:
### 7.2 Symbol im Zentrum
- Ist: Anfangsbuchstabe des Genres (z. B. „T"), 16px, Genre-Farbe.
- Soll:
### 7.3 Label-Platte (rechts angehängt)
- Ist: abgerundetes Rechteck rechts vom Kreis (~104×40), dunkel mit Genre-Rand + schwachem Glow.
- Soll:
### 7.4 Genre-Name
- Ist: Name in der Platte, hell (TEXT), ~12.5px.
- Soll:
### 7.5 BPM-Spanne
- Ist: „BPM 128–142" unter dem Namen, klein (~9px), bläulich-grau (TEXT_DIM).
- Soll:

## 8. Track-Knoten
### 8.1 Cover-Scheibe
- Ist: kreisförmig maskiertes Cover; echtes eingebettetes Albumcover bei Import, sonst generierter Genre-Sunset-Verlauf. Radius ~19 (aktiv 24).
- Soll:
### 8.2 Außenring + Glow
- Ist: dünner Genre-farbiger Ring mit weichem mehrlagigem Glühen; konzentrisch zum Cover.
- Soll:
### 8.3 Aktiv-Zusatzring (hervorgehobener Track)
- Ist: zusätzlicher roter Ring (bei aktivem Track), Radius+6 und +10, rot.
- Soll:
### 8.4 Track-Name
- Ist: unter dem Cover, hell (TEXT), ~9.5px.
- Soll:
### 8.5 BPM · Energie
- Ist: Zeile „138 BPM · E85" darunter, klein (~8px), bläulich-grau.
- Soll:

## 9. Beat-Ring (aktueller/laufender Track)
- Ist: konzentrischer Ring um das Cover (Radius+9); Helligkeit pulsiert zum Beat; kleiner Lichtpunkt läuft um den Ring; Phase aus echter Playback-Position + BPM. Knoten bleibt fix.
- Soll:

## 10. Pfeilspitzen
- Ist: kleine gefüllte Dreiecke am Zielrand jeder gerichteten Kante.
- Soll:

## 11. Kopfbereich über dem Baum
### 11.1 Titel
- Ist: „Dein Mix-Baum", 18px fett.
- Soll:
### 11.2 Untertitel
- Ist: „Themen verbinden. Übergänge entdecken. Den perfekten Flow finden.", 12px grau.
- Soll:
### 11.3 Legende
- Ist: „— Empfohlener Pfad" (rot), „— Alternativen" (cyan) + Hinweis „Klick = laden · Ziehen = schwenken · Rad = Zoom".
- Soll:

## 12. Farben der Genres (aktuell)
- Techno = Rot `#ff2d55` · House = Blau `#2d9cff` · Trance = Grün `#2ecc71` ·
  Ambient = Violett `#a855f7` · Hardstyle = Orange `#ff8c1a`.
- Soll:

---

### Nicht per egui erreichbar (bauartbedingt)
- Echter weicher Gauß-Blur (Glühen wird mehrlagig angenähert).
- Foto-Cover nur bei Import echter Dateien mit eingebettetem Bild; für Beispieldaten generierte Platzhalter.
