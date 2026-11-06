"""Static route facts from the player's scripts; never executes or exports Lua."""
import re

LONG = re.compile(r"\[(=*)\[")
TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]*|\d+(?:\.\d+)?|==|~=|>=|<=|\.\.")


def tokens(source):
    result = []
    at = 0
    while at < len(source):
        char = source[at]
        if char.isspace():
            at += 1
            continue
        comment = source.startswith("--", at)
        start = at + 2 if comment else at
        long = LONG.match(source, start)
        if long:
            close = "]" + long[1] + "]"
            end = source.find(close, start + len(long[0]))
            end = len(source) if end < 0 else end
            if not comment:
                result.append(("string", source[start + len(long[0]):end]))
            at = min(len(source), end + len(close))
        elif comment:
            end = source.find("\n", at)
            at = len(source) if end < 0 else end
        elif char in "\"'":
            at += 1
            value = []
            while at < len(source) and source[at] != char:
                if source[at] == "\\" and at + 1 < len(source):
                    at += 1
                    value.append({"n": "\n", "r": "\r", "t": "\t"}.get(source[at], source[at]))
                else:
                    value.append(source[at])
                at += 1
            at += 1
            result.append(("string", "".join(value)))
        else:
            match = TOKEN.match(source, at)
            value = match[0] if match else char
            result.append(("word", value))
            at += len(value)
    return result


def arguments(items, opening):
    args, current, depth = [], [], 0
    for at in range(opening + 1, len(items)):
        item = items[at]
        value = item[1] if item[0] == "word" else ""
        if value == ")" and depth == 0:
            args.append(current)
            return args
        if value == "," and depth == 0:
            args.append(current)
            current = []
        else:
            current.append(item)
            if value in ("(", "{", "["):
                depth += 1
            elif value in (")", "}", "]"):
                depth -= 1
    return []


def parse_script(name, source, text=None):
    text = text or {}
    items = tokens(source)
    values = [item[1] for item in items]
    variables, choices, bindings, stack, assigned = {}, {}, {}, [], set()
    links, missions, unknown = [], [], 0

    def literal(arg):
        if len(arg) != 1:
            return None
        return arg[0][1] if arg[0][0] == "string" else variables.get(arg[0][1])

    def expression(parts):
        raw = " ".join(('"' + value + '"') if kind == "string" else value for kind, value in parts)
        if len(parts) == 3 and parts[1][1] == "==" and parts[2][1].isdigit():
            variable, index = parts[0][1], parts[2][1]
            if variable in bindings:
                kind, target = bindings[variable]
                if kind == "choice" and index in choices.get(target, {}):
                    key = choices[target][index]
                    return "选项：" + text.get(key, key)
                if kind == "check":
                    return "检定 " + target + "：结果 " + index
        return raw

    at = 0
    while at < len(items):
        kind, value = items[at]
        if kind != "word":
            at += 1
            continue
        if value in ("if", "elseif"):
            end = at + 1
            while end < len(items) and items[end] != ("word", "then"):
                end += 1
            label = expression(items[at + 1:end])
            if value == "if":
                stack.append(("if", label))
            elif stack and stack[-1][0] == "if":
                stack[-1] = ("if", "否则若 " + label)
            at = end
        elif value == "else" and stack and stack[-1][0] == "if":
            stack[-1] = ("if", "其余分支（" + stack[-1][1] + "）")
        elif value in ("function", "do", "repeat"):
            stack.append((value, "函数体内" if value == "function" else "循环或代码块内"))
        elif value in ("end", "until") and stack:
            stack.pop()
        if at + 2 < len(items) and values[at + 1] == "=":
            if items[at + 2][0] == "string" and not stack and value not in assigned:
                variables[value] = items[at + 2][1]
            else:
                variables.pop(value, None)
            assigned.add(value)
            if values[at + 2:at + 4] == ["choose", "("] and at + 4 < len(items):
                bindings[value] = ("choice", values[at + 4])
            elif at + 6 < len(items) and values[at + 2] == "checkpointmanager" and values[at + 4] in ("Condition", "Switch"):
                args = arguments(items, at + 5)
                if args and literal(args[0]):
                    bindings[value] = ("check", literal(args[0]))
        if at + 5 < len(items) and values[at + 1] == "[" and values[at + 3:at + 5] == ["]", "="]:
            row = items[at + 5]
            if row[0] == "string":
                choices.setdefault(value, {})[values[at + 2]] = row[1]
            elif at + 7 < len(items) and values[at + 6] == "and" and items[at + 7][0] == "string":
                choices.setdefault(value, {})[values[at + 2]] = items[at + 7][1]
        if at + 3 < len(items) and values[at + 1] == "." and values[at + 3] == "(":
            method = values[at + 2]
            args = arguments(items, at + 3)
            label = " / ".join(label for _, label in stack if label)
            if value == "luamanager" and method in ("SetNextScript", "SetCurrentStoryScript"):
                target = literal(args[0]) if args else None
                if target and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", target) and not target.startswith("MOD_"):
                    links.append({"target": target, "condition": label})
                else:
                    unknown += 1
            elif value == "statmodifymanager" and method == "Mission" and len(args) >= 2:
                series, key = literal(args[0]), literal(args[1])
                if series and key:
                    missions.append({"series": series, "key": key, "condition": label})
                else:
                    unknown += 1
        at += 1
    return {"script": name, "links": links, "missions": missions, "unknown_jumps": unknown}


def read_routes(game):
    from vanilla_story_reader import resources, script_text, translations
    environment, entries = resources(game)
    sources = {pointer.read().m_Name: script_text(pointer) for path, pointer in entries.items()
               if path.startswith("story/chinesetraditional/")}
    texts = translations(game, "\n".join(sources.values()))
    return {"scripts": [parse_script(name, source, texts) for name, source in sorted(sources.items())]}
