# -*- coding: utf-8 -*-
"""Readable, DPI-aware controls embedded in Qt tables.

Qt's default table cell is often shorter/narrower than a native combo box at
125–200% scaling.  Centralising the sizing policy prevents the arrow button
from painting over text and lets the table scroll instead of clipping values.
"""

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QComboBox, QHeaderView, QTableWidget


MIN_ROW_HEIGHT = 34
MIN_COMBO_WIDTH = 96
MAX_COMBO_WIDTH = 420


def configure_table(table: QTableWidget) -> QTableWidget:
    table.setWordWrap(False)
    table.setHorizontalScrollMode(QTableWidget.ScrollMode.ScrollPerPixel)
    table.setVerticalScrollMode(QTableWidget.ScrollMode.ScrollPerPixel)
    table.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    table.verticalHeader().setDefaultSectionSize(MIN_ROW_HEIGHT)
    table.verticalHeader().setMinimumSectionSize(MIN_ROW_HEIGHT)
    table.horizontalHeader().setMinimumSectionSize(72)
    return table


def _combo_content_width(combo: QComboBox) -> int:
    metrics = combo.fontMetrics()
    widest = max(
        [metrics.horizontalAdvance(combo.currentText())]
        + [metrics.horizontalAdvance(combo.itemText(i)) for i in range(combo.count())]
    )
    # Text + frame + spacing + native arrow affordance.
    return max(MIN_COMBO_WIDTH, min(MAX_COMBO_WIDTH, widest + 44))


def configure_table_combo(
    table: QTableWidget, combo: QComboBox, column: int
) -> QComboBox:
    combo.setMinimumHeight(max(28, combo.fontMetrics().height() + 12))
    combo.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToContentsOnFirstShow)
    width = _combo_content_width(combo)
    combo.setMinimumWidth(width)
    combo.setToolTip(combo.currentText())
    combo.currentTextChanged.connect(combo.setToolTip)
    popup = combo.view()
    if popup is not None:
        popup.setMinimumWidth(width)
        popup.setTextElideMode(Qt.TextElideMode.ElideNone)
        popup.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    header = table.horizontalHeader()
    header.setSectionResizeMode(column, QHeaderView.ResizeMode.Interactive)
    table.setColumnWidth(column, max(table.columnWidth(column), width + 8))
    return combo


class ReadableTableWidget(QTableWidget):
    """QTableWidget that automatically sizes embedded combo boxes."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        configure_table(self)

    def setCellWidget(self, row, column, widget):  # noqa: N802
        super().setCellWidget(row, column, widget)
        if isinstance(widget, QComboBox):
            configure_table_combo(self, widget, column)
        self.setRowHeight(
            row, max(self.rowHeight(row), widget.minimumSizeHint().height() + 6)
        )
