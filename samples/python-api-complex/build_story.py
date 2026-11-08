"""Independent document-driven authoring of 雾渡夜灯; Python 3.10+, stdlib.

python make_wudu.py --editor PATH --sdk NEW_SDK_DIR --output NEW_RUN_DIR
Each run creates fresh outputs. No game, Host installation, or shared UI access.
"""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import zipfile


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--editor", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    exe, sdk, out = args.editor.resolve(), args.sdk.resolve(), args.output.resolve()
    if out.exists():
        raise FileExistsError(f"Use a NEW run directory: {out}")
    out.mkdir(parents=True)
    for key in ("APPDATA", "LOCALAPPDATA"):
        directory = out / key.lower()
        directory.mkdir()
        os.environ[key] = str(directory)
    log = out / "logs"
    log.mkdir()
    commands = []

    def raw(label, command):
        completed = subprocess.run(list(map(str, command)), capture_output=True, encoding="utf-8")
        (log / (label + ".stdout.txt")).write_text(completed.stdout, encoding="utf-8")
        (log / (label + ".stderr.txt")).write_text(completed.stderr, encoding="utf-8")
        commands.append({"label": label, "command": list(map(str, command)), "returncode": completed.returncode})
        write_json(out / "commands.json", commands)
        if completed.returncode:
            raise RuntimeError(f"{label}: exit {completed.returncode}; see {log}")
        return completed.stdout

    raw("sdk", [exe, "--cli", "sdk", "-o", sdk, "--json"])
    sys.path.insert(0, str(sdk))
    from lom_modkit import (CLI, Project, Chapter, Option, Say, Show, Portrait, Move,
                            Offset, Face, Dim, Focus, Hide, HideAll, Choice, Branch,
                            Flag, Parallel, Shock, Mood, Music, Sound, Background,
                            Overlay, CustomCg, Cg, Transition, Wait, End,
                            Task, Scenario, Event, Condition, All, Outcome, VariableChange,
                            Localization, Translation, ImageAsset, CharacterAsset, AudioAsset,
                            RouteTest, ChoiceStep)
    cli = CLI(exe)

    def call(label, *arguments):
        try:
            result = cli.run(*arguments)
        except Exception as error:
            write_json(log / (label + ".json"), getattr(error, "result", {"error": str(error)}))
            raise
        write_json(log / (label + ".json"), result)
        return result

    # The brief explicitly permits this original geometry/PCM generator.
    asset_tool = Path(__file__).with_name("prepare_assets.py")
    spec = importlib.util.spec_from_file_location("original_mist_assets", asset_tool)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    assets = module.create(out / "assets-source")
    project_assets = []
    for source in sorted((out / "assets-source").rglob("content.json")):
        meta = json.loads(source.read_text(encoding="utf-8"))
        common = dict(id=meta["id"], name=meta["name"], main=source.parent / meta["files"]["main"])
        if meta["type"] == "character":
            asset = CharacterAsset(**common, portraits={k: source.parent / v for k, v in meta["portraits"].items()})
        elif meta["type"] == "audio":
            asset = AudioAsset(**common, audio_kind=meta["audio_kind"])
        else:
            asset = ImageAsset(**common)
        project_assets.append(asset)
    schema = call("schema-nodes", "schema", "nodes")
    catalog = call("schema-catalog", "schema", "catalog")
    assert "M" in {p["id"] for p in catalog["positions"]}
    assert "player" in {p["id"] for p in catalog["characters"]}
    shen = "user:mist.shen"
    harbor, tower, letter = ("user:mist." + key for key in ("harbor", "tower", "letter"))
    theme, bell = "user:mist.theme", "user:mist.bell"
    def n(id, text, **kwargs):
        return Say(id=id, mode="narrative", text=text, **kwargs)
    def s(id, text, **kwargs):
        return Say(id=id, character=shen, text=text, **kwargs)
    def p(id, text, **kwargs):
        return Say(id=id, character="player", text=text, **kwargs)
    def choice(id, entries):
        return Choice(id=id, options=[Option(text=text, goto=target) for text, target in entries])
    def flag_branch(id, flag, yes, no):
        return Branch(id=id, source="mod", flag=flag,
                      cases=[{"value": 1, "goto": yes}, {"value": 2, "goto": no}])
    def entry(bg, portrait="normal"):
        return [Background(id="background", action="set", image=bg),
                Show(id="shen_enter", character=shen, position="R1", portrait=portrait),
                Show(id="traveler_enter", character="player", position="L1", portrait="normal")]

    chapters = []
    chapters.append(Chapter(id="main", title="雾渡来信", nodes=[
        Transition(id="cover", phase="in", dir="lr"),
        Background(id="night", action="set", image=harbor),
        Music(id="theme", name=theme),
        Transition(id="curtain", phase="out", dir="lr"),
        n("opening", "这座镇子靠一盏灯认得回家的方向，今夜灯却又灭了。"),
        n("rain", "雨停在瓦上，渡口仍在下雨。旅人掀开湿透的斗笠，看见堤岸上一点倔强的火星。"),
        Show(id="traveler", character="player", position="L1", portrait="normal"),
        Show(id="shen", character=shen, position="R1", portrait="normal", fadeDuration=0.4),
        Offset(id="oil_sleeve", character=shen, x=-25, y=0, duration=0.2, absolute=True, scale=100),
        n("frame", "年轻的守灯人抱着灯架，黑油从衣袖滴下来，落进积水却没有散开。"),
        p("question", "灯灭了三次，你却还在这里修它？"),
        Focus(id="focus", character=shen),
        s("responsibility", "灯是我看着的，错也该由我担。只是账上的油，不该只剩这一点。"),
        Sound(id="distant_bell", name=bell),
        Overlay(id="letter_show", action="show", slot="letter", image=letter, position="center", scale=80, fade=0.3),
        n("letter_words", "九月初三，灯油十二坛；实际入库，八坛。水把落款泡成一团，两个数字却清清楚楚。"),
        choice("letter_action", [("先记住十二与八的差额。", "accept_job"), ("先记住账单上的日期。", "accept_job")]),
        choice("accept_job", [("留下，替他查账。", "accept"), ("潮要涨了，今夜先走。", "refuse")]),
        Overlay(id="accept", action="hide", slot="letter", fade=0.2),
        p("promise", "我不替谁认罪，也不替谁脱罪。先看看这四坛油去了哪里。"),
        s("market_hint", "集市还有一盏药铺灯。昨夜摆渡的人，也许还没走。"),
        End(id="to_market", next_script="market"),
        Overlay(id="refuse", action="hide", slot="letter", fade=0.2),
        p("refusal_reason", "船不等人，我只能把这张账单留给你。愿下一盏灯能撑到天亮。"),
        s("refusal_reply", "路上小心。今晚看不见灯，就听钟声。"),
        End(id="to_left", next_script="ending_left"),
    ], localization=Localization(default_locale="chs", fallback_locale="cht", translations={
        "cht": {"opening.text": "這座鎮子靠一盞燈認得回家的方向，今夜燈卻又滅了。", "accept_job.options.0.text": "留下，替他查帳。"},
        "ja": {"opening.text": "この町は一つの灯を頼りに帰り道を見つける。だが今夜、その灯がまた消えた。", "accept_job.options.0.text": "ここに残り、彼の帳簿を調べる。"}
    })))
    chapters.append(Chapter(id="market", title="未打烊的集市", nodes=entry(harbor) + [
        n("market_arrival", "棚布兜住半天雨水，药香与油腥混在一起。摆渡人正把断桨靠到墙边。"),
        s("ask_where", "你想先问人，还是先看坛子？两处都在，路可别走急了。"),
        choice("market_action", [("问摆渡人昨夜的钟声。", "ask_bell"), ("检查药铺背后的油坛。", "check_oil"), ("带着线索去旧桥。", "leave_market")]),
        Focus(id="ask_bell", character="player"),
        p("bell_question", "昨夜第一声钟响时，你在哪里？"),
        n("ferryman", "摆渡人把桨举起来：第一遍钟响时灯还亮着，第三遍才灭。守灯人那时在我船上，替我绑断了的桨。"),
        s("recognition", "绳结还在。你若愿作证，今夜就少一个人白白挨骂。"),
        Flag(id="heard", flag="heard_bell"),
        n("bell_return", "旅人将三遍钟声的先后默记在心，转回药铺门前。", goto="market_action"),
        Face(id="check_oil", character=shen, facing="left"),
        n("oil_stain", "药铺背后八只坛子排成两行，第八只坛底垫着一页折过的纸。"),
        Parallel(id="oil_reveal", actions=[Sound(name=bell).action(), Overlay(action="show", slot="evidence", image=letter, fade=0.4, scale=75).action()]),
        p("oil_read", "进项写十二，交割却写八。少的不是算珠，是四坛实物。"),
        Portrait(id="relief", character=shen, portrait="normal", fade_out=0.1, fade_in=0.2),
        s("not_mistaken", "原来不是我记错了。"),
        Flag(id="kept", flag="kept_letter"),
        Overlay(id="put_away", action="hide", slot="evidence", fade=0.3),
        n("oil_return", "旅人用干布包好账单。问话的地方仍有灯，何时离开由他自己决定。", goto="market_action"),
        n("leave_market", "集市最后一扇门合上了，旧桥那边传来木头受力的低响。"),
        p("take_care", "证据可以慢慢凑，潮水不会等。走，先看桥。"),
        End(id="to_crossing", next_script="crossing"),
    ]))
    chapters.append(Chapter(id="crossing", title="潮过旧桥", nodes=entry(harbor, "angry") + [
        n("bridge", "旧桥在脚下震动。桥下有条空船被粗绳拴死，水面已经高过最低一道桥板。"),
        s("reckless", "我只知道修那盏灯，竟没来看看桥！若桥断了，谁还能走？"),
        Move(id="angry_move", character=shen, from_="R1", to="M", duration=0.6),
        Parallel(id="shock_and_mood", actions=[Shock(character=shen, duration=0.7).action(), Mood(enabled=True, character=shen, icon="shock", offset_x=8, offset_y=-5).action()]),
        n("rope", "又一声裂响从桥腹钻出。沈砚仍皱着眉，目光却从灯塔移向那条空船。"),
        Mood(id="settle", enabled=False, character=shen),
        Parallel(id="move_and_mood", actions=[Move(character=shen, from_="M", to="R1", duration=0.5).action(), Mood(enabled=True, character=shen, icon="nervous1").action()]),
        p("assess", "船能救急，灯塔里也许藏着答案。两件都要做，就得先排个顺序。"),
        choice("bridge_action", [("先松船绳，给镇里留条退路。", "free_rope"), ("带着线索直上灯塔。", "hurry")]),
        n("free_rope", "旅人蹲下解开死结，沈砚拉住船头，把缆绳改系到尚牢固的石桩上。"),
        Flag(id="freed", flag="boat_freed"),
        s("rope_reply", "这条绳子今夜不拴船，拴人心。", goto="join"),
        p("hurry", "先去灯塔。等问明白，我们再回头处理船绳。"),
        s("hurry_reply", "但愿桥还肯等我们。"),
        Mood(id="join", enabled=False, character=shen),
        Face(id="face_tower", character=shen, facing="right"),
        n("stairs", "石阶上有一条新鲜油痕，顺着雨水流向塔门。"),
        Hide(id="leave_shen", character=shen, fadeDuration=0.2),
        End(id="to_tower", next_script="tower"),
    ]))
    chapters.append(Chapter(id="tower", title="灯下对质", nodes=entry(tower) + [
        n("manager", "管事隔着门说，账册没有错，错的是守灯人的手。旅人把灯放在门槛上。"),
        Dim(id="dim_shen", character=shen, dimmed=True),
        p("confront", "手会抖，账却不会自己少四坛。先让人把话说完。"),
        flag_branch("evidence_bell", "heard_bell", "evidence_letter", "no_bell"),
        flag_branch("evidence_letter", "kept_letter", "all_evidence", "no_letter"),
        p("all_evidence", "摆渡人能证明他在船上，坛底账单能证明油未足额入库。你要解释的是两件事。", goto="evidence_join"),
        p("no_bell", "没有人能证明守灯人昨夜去了哪里。光凭油数，我还不能断言是谁动了手脚。"),
        flag_branch("missing_letter_too", "kept_letter", "only_letter", "neither"),
        s("only_letter", "账单虽在，可我的行踪还得有人作证。", goto="evidence_join"),
        p("neither", "连交割账单也没有。现在只有传言，谁的名字都不能轻易写上。", goto="evidence_join"),
        p("no_letter", "摆渡人证明了他的行踪，但缺少交割账单，还不能把四坛油落到某个人名下。"),
        Dim(id="evidence_join", character=shen, dimmed=False),
        s("human_cost", "桥快撑不住了。我可以等一个清白，桥上的人等不起。"),
        Sound(id="warning_bell", name=bell),
        choice("tower_action", [("先护住镇上的人。", "check_boat"), ("当众说出账本上的名字。", "check_bell")]),
        flag_branch("check_boat", "boat_freed", "save_ready", "no_boat"),
        p("save_ready", "船已松好。叫大家沿石阶下去，先过这一夜。"),
        End(id="to_saved", next_script="ending_saved"),
        p("no_boat", "船还拴在桥下，等我们折回去已经太迟。救人的话说了，却没给人留下路。"),
        End(id="boat_failure", next_script="ending_left"),
        flag_branch("check_bell", "heard_bell", "check_letter", "accuse_no_bell"),
        flag_branch("check_letter", "kept_letter", "accuse_ready", "accuse_no_letter"),
        p("accuse_ready", "请摆渡人进来，再把这张入库单当众念一遍。名字在纸上，责任也该站到灯下。"),
        End(id="to_exposed", next_script="ending_exposed"),
        p("accuse_no_bell", "我说出了名字，却答不出守灯人离塔的时辰。指控没有证人支撑，围观的人散了。"),
        flag_branch("accuse_missing_both", "kept_letter", "failed_departure", "accuse_no_paper"),
        p("accuse_no_paper", "手里连账单也没有，十二与八仍只是我说过的两个数字。"),
        End(id="failed_departure", next_script="ending_left"),
        p("accuse_no_letter", "证人能替他洗去一半嫌疑，却不能替我拿出失踪的交割单。这次指控缺了最要紧的一页。"),
        End(id="letter_failure", next_script="ending_left"),
    ]))
    chapters.append(Chapter(id="ending_saved", title="一船灯火", nodes=entry(harbor) + [
        n("boat_catches", "桥板塌下时，空船正好靠住石阶。避雨的人一个接一个踏进船舱，没有人再问灯油。"),
        CustomCg(id="rescue_cg", action="show", image=harbor, fade=0.5),
        s("rescue_words", "账可以明日再算，人不能明日再救。"),
        p("remember", "你记着这句话，也别忘了账。"),
        n("hands", "船上的手依次接过灯架，火光绕着湿衣袖转了一圈，又回到沈砚手上。"),
        CustomCg(id="rescue_hide", action="hide", fade=0.3),
        Music(id="rescue_fade", name=theme, op="fadeout", seconds=0.8),
        HideAll(id="rescue_exit", fadeDuration=0.3),
        n("safe_dawn", "天将亮时，渡口多了一船人，也多了一件明日必须算清的事。"),
        End(id="rescued_end", event_result="rescued"),
    ]))
    chapters.append(Chapter(id="ending_exposed", title="灯照旧账", nodes=entry(tower) + [
        Cg(id="ledger", action="show", kind="event", key=letter, event_title="灯下旧账", event_text="入库八坛，账记十二。", fade_in=0.3),
        n("found_oil", "账本摊在灯下，巡夜人从管事库房找出藏起的四坛油。封口上的红线，还与药铺的一样。"),
        s("thought", "我以为熬过今夜就好。"),
        p("next_night", "把今夜说清楚，明夜才不会重来。"),
        Cg(id="ledger_hide", action="hide", kind="event"),
        Portrait(id="happy", character=shen, portrait="happy", fade_out=0.2, fade_in=0.4),
        Mood(id="happy_bubble", enabled=True, character=shen, icon="laugh1"),
        n("relit", "沈砚抹净灯口，灯火重新亮起来。照到的不只河面，还有每个人不愿看的那页账。"),
        HideAll(id="exposed_exit", fadeDuration=0.3),
        Music(id="exposed_fade", name=theme, op="fadeout", seconds=0.6),
        End(id="exposed_end", event_result="exposed"),
    ]))
    chapters.append(Chapter(id="ending_left", title="潮声照旧", nodes=entry(harbor) + [
        n("departure", "旅人走下渡口。身后仍有敲打灯架的声音，一轻一重，像有人在雨里补写一句话。"),
        s("keep_working", "灯总得有人修。你走吧，水还没漫过这块石头。"),
        p("farewell", "路若再通，我还会回来。"),
        Face(id="turn", character="player", facing="left"),
        Hide(id="traveler_exit", character="player", fadeDuration=0.3),
        Focus(id="last_light", character=shen),
        n("rumors", "天亮后镇上仍会说起昨夜，只是每个人说的都不一样。"),
        Wait(id="listen", seconds=0.35),
        Music(id="left_fade", name=theme, op="fadeout", seconds=0.6),
        HideAll(id="left_exit", fadeDuration=0.3),
        End(id="left_end", event_result="left"),
    ]))
    chapters.append(Chapter(id="epilogue", title="翌日回信", nodes=entry(harbor, "happy") + [
        n("reply_arrives", "翌日雨歇，一封干燥的回信放在修好的灯架旁。镇上终于肯让守灯人把字写完。"),
        s("return_frame", "灯架的铜脚补好了。昨日借你的那块干布，也洗净晾过。"),
        p("keep_ledger", "这回的账，你自己收好。"),
        Overlay(id="reply_show", action="show", slot="reply", image=letter, position="center", scale=75, fade=0.4),
        n("reply_text", "信上没有谢字，只有四栏写得极端齐整的数：收、存、支、余。最下面添了一句：灯已点。"),
        s("new_habit", "往后每开一坛，我都记一笔。人多看一眼，比我多担一次错有用。"),
        Mood(id="reply_mood", enabled=True, character=shen, icon="shy"),
        p("answer", "那我下次再来，就只问灯亮不亮。"),
        Overlay(id="reply_hide", action="hide", slot="reply", fade=0.3),
        HideAll(id="reply_exit", fadeDuration=0.3),
        End(id="reply_end", event_result="completed"),
    ]))
    chapters.append(Chapter(id="daily_bridge", title="修桥小记", nodes=entry(harbor) + [
        n("bridge_work", "木匠把新桥板一块块递过去，沈砚守着绳头。河水退了，桥下仍留着昨夜的泥线。"),
        p("bridge_question", "今日还要松船绳么？"),
        s("bridge_answer", "每天检查一遍。留路这件事，不能只在出事时想起来。"),
        Portrait(id="bridge_smile", character=shen, portrait="happy", fade_out=0.1, fade_in=0.2),
        Sound(id="work_bell", name=bell),
        n("bridge_note", "旅人替他扶住一块木板，看见绳结已经换成一拉便开的活扣。"),
        p("bridge_promise", "下回来时，我可要从新桥上走。"),
        HideAll(id="bridge_exit", fadeDuration=0.2),
        End(id="bridge_daily_end", event_result="completed"),
    ]))
    chapters.append(Chapter(id="daily_lamp", title="添灯小记", nodes=entry(tower) + [
        n("lamp_work", "傍晚，沈砚用小铜勺添油。每一勺都先停在灯下，像在认真量一段将来的夜路。"),
        p("lamp_question", "今天数过几回了？"),
        s("lamp_answer", "数给账看一回，数给自己看一回。剩下一回，留给路过的人。"),
        Portrait(id="lamp_smile", character=shen, portrait="happy", fade_out=0.1, fade_in=0.2),
        Mood(id="lamp_mood", enabled=True, character=shen, icon="laugh1"),
        n("lamp_note", "火苗稳稳站起来，没有比昨夜更亮，却不再摇得那么慌张。"),
        p("lamp_reply", "够了。有人抬头的时候，能看见就够了。"),
        HideAll(id="lamp_exit", fadeDuration=0.2),
        End(id="lamp_daily_end", event_result="completed"),
    ]))

    def task_condition(key, value):
        return {"source": "task", "key": key, "op": "eq", "value": value}
    def outcome(task, harbor_state=None):
        tasks = {"investigation": task}
        if harbor_state:
            tasks["harbor"] = harbor_state
        return {"tasks": tasks}
    events = [
        Event(id="mist_start", name="雾渡来信", script="main", trigger="start", outcomes={
            "completed": Outcome(), "rescued": Outcome(tasks={"investigation": "resolved", "harbor": "safe"}),
            "exposed": Outcome(tasks={"investigation": "resolved"}), "left": Outcome(tasks={"investigation": "abandoned"})}),
        Event(id="mist_reply", name="翌日回信", trigger="position", position="Center", script="epilogue",
            when=All(Condition(source="task", key="investigation", op="eq", value="resolved"),
                     Condition(source="variable", key="letters_received", op="eq", value=0)),
            outcomes={"completed": Outcome(tasks={"investigation": "thanked"}, variables={"letters_received": VariableChange(op="add", value=1)})})]
    for ident, name, weight in [("daily_bridge", "修桥小记", 30), ("daily_lamp", "添灯小记", 70)]:
        events.append(Event(id=ident, name=name, trigger="position", position="Center", script=ident,
            once=False, check="action", pool="mist_daily", weight=weight,
            when=Condition(source="task", key="investigation", op="eq", value="thanked"),
            outcomes={"completed": Outcome(variables={"daily_visits": VariableChange(op="add", value=1)})}))
    scenario = Scenario(entry_event="mist_start", tasks=[
        Task(id="investigation", name="雾渡灯油调查", states=["active", "resolved", "thanked", "abandoned"], initial="active"),
        Task(id="harbor", name="渡口退路", states=["unsafe", "safe"], initial="unsafe")], events=events)

    routes = [
        ("救人_完整调查", [0, 1, 2], 0, 0, "rescued_end", {"heard_bell": True, "kept_letter": True, "boat_freed": True}),
        ("揭穿_完整调查", [0, 1, 2], 1, 1, "exposed_end", {"heard_bell": True, "kept_letter": True}),
        ("指控_无证据", [2], 1, 1, "left_end", {}),
        ("护人_未松船", [0, 1, 2], 1, 0, "left_end", {"heard_bell": True, "kept_letter": True}),
        ("开场拒绝", None, None, None, "left_end", {}),
        ("逆序调查_揭穿", [1, 0, 2], 0, 1, "exposed_end", {"heard_bell": True, "kept_letter": True, "boat_freed": True}),
        ("只有证人_指控", [0, 2], 1, 1, "left_end", {"heard_bell": True}),
        ("只有账单_指控", [1, 2], 1, 1, "left_end", {"kept_letter": True}),
    ]
    tests = []
    for name, market, bridge, final, end, flags in routes:
        choices = [{"node": "letter_action", "option": 0}, {"node": "accept_job", "option": 1 if market is None else 0}]
        if market is not None:
            choices += [{"node": "market_action", "option": x} for x in market]
            choices += [{"node": "bridge_action", "option": bridge}, {"node": "tower_action", "option": final}]
        tests.append(RouteTest(name=name, story="main", choices=[ChoiceStep(**step) for step in choices], reaches_node=[end], flags=flags))
    chapters[0].metadata["_editor"] = {"tests": tests}
    project = Project(id="mist_night_independent", name="雾渡夜灯", author="独立 Python API 验收", version="1.0.0",
                      chapters=chapters, mode="sandbox", scenario=scenario, assets=project_assets,
                      manifest={"description": "秋雨后的灯油调查；自有几何示意素材与合成音频。"})
    project.write(out / "request.json")
    counts = {c.id: len(c.nodes) for c in chapters}
    kinds = sorted({node["type"] for c in chapters for node in c.nodes})
    write_json(out / "coverage.json", {"chapters": counts, "nodes": sum(counts.values()), "node_types": kinds, "node_type_count": len(kinds)})
    assert len(chapters) == 10 and sum(counts.values()) >= 100 and len(kinds) >= 22
    try:
        result = cli.build(project, out / "project")
    except Exception as error:
        write_json(log / "build.json", getattr(error, "result", {"error": str(error)}))
        raise
    write_json(log / "build.json", result)
    call("texts", "texts", out / "project/story/main.json")
    call("preflight", "preflight", out / "project", "--profile", "release")
    call("statistics", "statistics", out / "project")
    test_result = call("routes", "test", out / "project")
    assert len(test_result["results"]) == len(routes)
    assert all(r["status"] == "pass" for r in test_result["results"])
    # Independent trace checks prevent a malformed/ignored assertion from masquerading as acceptance.
    for result, route in zip(test_result["results"], routes):
        visited = result["visited"]
        assert visited[-1][1] == route[4], (result, route)
        market_choices = [v for v in visited if v == ["market", "market_action"]]
        assert len(market_choices) == len(route[1] or []), result
        assert all(bool(result["flags"].get(key)) == value for key, value in route[5].items()), result
        if route[1] == [0, 2]:
            assert ["market", "check_oil"] not in visited, result
        if route[1] == [1, 2]:
            assert ["market", "ask_bell"] not in visited, result
    write_json(out / "trace-assertions.json", {"passed": len(routes), "checks": ["exact final node", "loop visit count", "expected flags", "no investigation fallthrough"]})
    # Negative control: a genuinely wrong native assertion must fail.
    probe = Chapter(id="probe", title="断言格式复现", nodes=[
        choice("pick", [("甲", "a"), ("乙", "b")]), End(id="a"), End(id="b")],
        _editor={"tests": [{"name": "wrong-end", "story": "probe", "actions": {"choices": [{"node": "pick", "option": 1}]},
                            "assert": {"reaches_node": ["a"], "reaches_ending": True}}]}).to_dict()
    write_json(out / "probe-native-assert.json", probe)
    try:
        negative = cli.run("test", out / "probe-native-assert.json")
    except Exception as error:
        negative = getattr(error, "result", {"error": str(error)})
    write_json(log / "probe-native-assert.json", negative)
    assert negative["results"][0]["status"] == "fail", negative
    probe["_editor"]["tests"][0]["expect"] = probe["_editor"]["tests"][0].pop("assert")
    write_json(out / "probe-unknown-expect.json", probe)
    try:
        cli.run("test", out / "probe-unknown-expect.json")
    except Exception as error:
        unknown = getattr(error, "result", {"error": str(error)})
        write_json(log / "probe-unknown-expect.json", unknown)
        assert unknown["ok"] is False and "expect" in str(unknown)
    else:
        raise AssertionError("Unknown expectation must be rejected")
    write_json(log / "export.json", cli.export(out / "project", out / "雾渡夜灯.lommod"))
    call("inspect", "inspect", out / "雾渡夜灯.lommod")
    for locale in ("cht", "ja"):
        call("compile-" + locale, "compile", out / "project/story/main.json", "-o", out / ("main-" + locale + ".lua"), "--locale", locale)
    raw("smoke-project", [exe, "--smoke-preview", out / "project"])
    raw("smoke-package", [exe, "--smoke-preview", out / "雾渡夜灯.lommod"])

    original = Project.open(cli, out / "雾渡夜灯.lommod")
    write_json(out / "package-read.json", original.recipe)
    identity = original.manifest["campaign_id"]
    def package_assets(path):
        with zipfile.ZipFile(path) as archive:
            return {name: hashlib.sha256(archive.read(name)).hexdigest()
                    for name in archive.namelist() if name.startswith("assets/")}
    before_assets = package_assets(out / "雾渡夜灯.lommod")
    target = next(x for x in original.stories["epilogue"]["nodes"] if x["id"] == "answer")
    target["text"] = "那我下次再来，就只问灯亮不亮。桥若修好，也请替我留一步。"
    original.manifest["version"] = "1.0.1"
    original.write(out / "edited-request.json")
    write_json(log / "build-edited.json", cli.build(original, out / "project-edited"))
    write_json(log / "export-edited.json", cli.export(out / "project-edited", out / "雾渡夜灯_修订.lommod"))
    reopened = Project.open(cli, out / "雾渡夜灯_修订.lommod")
    write_json(out / "edited-package-read.json", reopened.recipe)
    after_assets = package_assets(out / "雾渡夜灯_修订.lommod")
    assert reopened.manifest["campaign_id"] == identity
    assert before_assets == after_assets
    assert reopened.recipe["scenario"] == original.recipe["scenario"]
    assert next(x for x in reopened.stories["epilogue"]["nodes"] if x["id"] == "answer")["text"] == target["text"]
    call("inspect-edited", "inspect", out / "雾渡夜灯_修订.lommod")
    call("test-edited", "test", out / "project-edited")
    raw("smoke-edited", [exe, "--smoke-preview", out / "雾渡夜灯_修订.lommod"])
    with zipfile.ZipFile(out / "雾渡夜灯.lommod") as z:
        exported_assets = {name: hashlib.sha256(z.read(name)).hexdigest() for name in z.namelist() if name.startswith("assets/")}
    assert set(exported_assets) == set(assets)
    normalizations = []
    with zipfile.ZipFile(out / "雾渡夜灯.lommod") as archive:
        for key, path in assets.items():
            src = Path(path).read_bytes()
            if key.endswith(".json"):
                expected, actual = json.loads(src), json.loads(archive.read(key))
                if expected.get("type") == "character":
                    expected.pop("scale", None); expected.pop("art_facing", None); expected.pop("mood_x", None); expected.pop("mood_y", None)
                if expected.get("type") == "character":
                    for field, default in {"scale": 100, "art_facing": "left", "mood_x": 50, "mood_y": 90}.items():
                        if field in expected and field not in actual:
                            assert expected[field] == default
                            normalizations.append({"path": key, "omitted_default": field, "value": default})
                            expected.pop(field)
                assert expected == actual, key
            else:
                assert exported_assets[key] == hashlib.sha256(src).hexdigest(), key
    write_json(out / "asset-default-normalization.json", normalizations)
    write_json(out / "roundtrip.json", {"campaign_id": identity, "campaign_id_preserved": True, "assets_preserved": True,
        "asset_count": len(after_assets), "scenario_preserved": True, "dialogue_edit_preserved": True,
        "original_binary_assets_match_package": True, "metadata_equal_after_default_elision": True,
        "save_namespace": "mod_campaign_" + identity,
        "actual_game_saves_touched": False})
    print(json.dumps({"ok": True, "output": str(out), "nodes": sum(counts.values()), "node_types": len(kinds)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
