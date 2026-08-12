# -*- coding: utf-8 -*-
"""lomc 错误类型。"""


class LomcError(Exception):
    """编译/校验/打包过程中的用户可读错误。

    message 一律为简体中文，尽量带节点 id 或字段名，方便 mod 作者定位。
    """
