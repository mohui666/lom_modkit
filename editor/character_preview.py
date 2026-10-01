"""A full-height portrait inspector sharing the stage's asset resolver and LRU."""
from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QColor, QPainter

from i18n import t
import models
from preview import StagePreview


class CharacterPortraitPreview(StagePreview):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.character_id = ""
        self.portrait_id = "normal"
        self.appearance = ""
        self.setObjectName("characterPortraitPreview")

    def set_character(self, character: str, portrait: str = "normal", appearance: str = "") -> None:
        self.character_id, self.portrait_id, self.appearance = character, portrait, appearance
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.fillRect(self.rect(), QColor(18, 21, 31, 225))
        painter.setPen(QColor(242, 242, 247))
        if not self.character_id:
            painter.drawText(self.rect().adjusted(24, 24, -24, -24), Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap, t("portrait.choose_character"))
            return
        title = models.character_name(self._editor_data, self.character_id)
        if self.appearance == "beautified":
            title = t("portrait.beautified_player")
        painter.drawText(QRectF(20, 16, self.width()-40, 28), title)
        painter.setPen(QColor(178, 185, 205))
        painter.drawText(QRectF(20, 47, self.width()-40, 24), "%s · %s" % (self.character_id, self.portrait_id))
        lookup_id = "player_beautified" if self.character_id == "player" and self.appearance == "beautified" else self.character_id
        image = self._load_pixmap(self._portrait_path(lookup_id, self.portrait_id))
        area = QRectF(20, 83, max(1, self.width()-40), max(1, self.height()-125))
        if image.isNull():
            painter.drawText(area, Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap, t("portrait.no_assets"))
        else:
            ratio = min(area.width()/image.width(), area.height()/image.height())
            size = (image.width()*ratio, image.height()*ratio)
            target = QRectF(area.center().x()-size[0]/2, area.center().y()-size[1]/2, *size)
            painter.drawPixmap(target, image, QRectF(image.rect()))
        painter.setPen(QColor(178, 185, 205))
        painter.drawText(QRectF(20, self.height()-31, self.width()-40, 22), t("portrait.private_assets"))
