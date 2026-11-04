"""Export the native StoryMainUI book frame into the user's preview cache."""
import UnityPy


def extract_event_book(data, output):
    environment = UnityPy.load(str(data / "sharedassets2.assets"))
    sprites = [obj.read() for obj in environment.objects if obj.type.name == "Sprite"]
    books = [sprite for sprite in sprites if sprite.m_Name == "pic_book_1"]
    if len(books) != 1:
        raise ValueError("无法唯一定位原版事件书页 pic_book_1")
    relative = "assets/ui/event_book.png"
    path = output / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    books[0].image.save(path)
    return relative
