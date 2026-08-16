# Screenshot detector corpus

`tests.test_watermark_detector` deterministically generates this corpus in a
temporary directory so the repository does not carry six large derived image
files. The source scene seed, dimensions and every transform are fixed:

| case | format / transform | expected |
|---|---|---|
| original | PNG | detected |
| jpeg | JPEG quality 85 | detected |
| resize | bicubic 75% | detected |
| mild_crop | crop L37/T23/R29/B17 | detected |
| brightness | brightness 1.12 | detected |
| contrast | contrast 0.85 | detected |
| clean_negative | same scene without carrier | not detected |

The corpus is synthetic and verifies deterministic regression behavior. It is
not a substitute for screenshots captured from 《活侠传》; real-game capture
validation remains a separate manual acceptance item.
