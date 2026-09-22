#!/usr/bin/env python3
"""导入流光演示数据。

用法:
    python3 seed/import.py [http://localhost:8080]
"""

import json
import mimetypes
import subprocess
import sys
from pathlib import Path

API = sys.argv[1].rstrip("/") if len(sys.argv) > 1 else "http://localhost:8080"
ROOT = Path(__file__).resolve().parent


def find_image(keyword: str) -> Path | None:
    matches = sorted(ROOT.glob(f"*{keyword}*.png")) + sorted(ROOT.glob(f"*{keyword}*.jpg"))
    return matches[0] if matches else None


def image_size(path: Path) -> tuple[int | None, int | None]:
    """有 Pillow 就按真实尺寸上报，没有则退回到手写的参考值。"""
    try:
        from PIL import Image

        with Image.open(path) as image:
            return image.size
    except Exception:
        return (None, None)


def upload(path: Path, kind: str, width: int | None = None, height: int | None = None) -> str:
    mime = mimetypes.guess_type(path.name)[0] or "image/png"
    cmd = [
        "curl", "-s", "-X", "POST", f"{API}/api/media/upload/{kind}",
        "-F", f"file=@{path};type={mime}",
    ]
    if width:
        cmd += ["-F", f"width={width}"]
    if height:
        cmd += ["-F", f"height={height}"]

    raw = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    payload = json.loads(raw)
    if "id" not in payload:
        raise RuntimeError(f"上传失败: {payload}")
    return payload["id"]


def create_moment(payload: dict) -> dict:
    raw = subprocess.run(
        [
            "curl", "-s", "-X", "POST", f"{API}/api/moments",
            "-H", "Content-Type: application/json",
            "-d", json.dumps(payload, ensure_ascii=False),
        ],
        capture_output=True, text=True, check=True,
    ).stdout
    result = json.loads(raw)
    if "id" not in result:
        raise RuntimeError(f"创建失败: {result}")
    return result


# keyword 对应 seed/ 目录下的图片文件名
MOMENTS = [
    {
        "title": "傍晚的河堤",
        "content": "风很软，天边烧了一层橘色。沿着河走了很久，什么也没想，"
                   "只是看着水面上碎掉的光一点点暗下去。",
        "tags": ["散步", "傍晚", "河"],
        "at": "2026-09-13T11:30:00Z",
        "image": "river",
        "dim": (1536, 973),
    },
    {
        "title": "窗边的下午",
        "content": "三点钟的光斜斜地照进来，桌上有一小片暖。\n"
                   "点了最普通的一杯拿铁，看了半本书，剩下的半本留给下次。",
        "tags": ["咖啡", "独处"],
        "at": "2026-09-08T07:10:00Z",
        "image": "coffee",
        "dim": (1024, 973),
    },
    {
        "title": "雾里的山",
        "content": "六点就上山了。雾很厚，松树一棵一棵地消失在白色里，"
                   "像有人在前面慢慢把画擦掉。走到半山腰的时候，只听得见自己的呼吸。",
        "tags": ["徒步", "山", "清晨"],
        "at": "2026-08-24T22:40:00Z",
        "image": "misty",
        "dim": (1024, 1460),
    },
    {
        "title": "它睡了一下午",
        "content": "一整个下午都趴在同一格书架上，换了三个姿势。\n"
                   "我路过四次，它睁了一次眼。",
        "tags": ["猫", "家", "日常"],
        "at": "2026-08-02T13:05:00Z",
        "image": "orange",
        "dim": (1024, 973),
    },
    {
        "title": "什么也没做的一天",
        "content": "睡到中午，煮了面，把阳台的花浇了，又躺回沙发。\n"
                   "好像什么都没做，但心里是满的。",
        "tags": ["日常", "休息"],
        "at": "2026-06-18T15:12:00Z",
        "image": None,
    },
    {
        "title": "跨年的那一刻",
        "content": "没有倒数，也没有烟花。\n"
                   "只是站在窗前，看着对面楼里一格一格的灯，想：又一年过去了。",
        "tags": ["跨年", "夜"],
        "at": "2025-12-31T15:50:00Z",
        "image": None,
    },
    {
        "title": "搬进新家的第一天",
        "content": "纸箱堆到天花板，只有一张床是收拾好的。\n"
                   "半夜坐在箱子上吃外卖，觉得这个城市好像也有了我的一小块地方。",
        "tags": ["搬家", "新开始"],
        "at": "2025-11-09T14:20:00Z",
        "image": None,
    },
    {
        "title": "第一次在这里留下点什么",
        "content": "决定开始记下一些事。\n不需要完整，不需要有意义，只是不想让它们就这么过去了。",
        "tags": ["开始"],
        "at": "2025-08-16T10:30:00Z",
        "image": None,
    },
]


def main() -> None:
    print(f"→ 目标服务 {API}")

    media_cache: dict[str, str] = {}
    created = 0

    for item in MOMENTS:
        keyword = item.get("image")
        media_ids: list[dict] = []

        if keyword:
            path = find_image(str(keyword))
            if path is None:
                print(f"  ! 找不到关键词为 {keyword} 的图片，跳过这张")
            else:
                if keyword not in media_cache:
                    width, height = image_size(path)
                    if width is None:
                        width, height = item.get("dim", (None, None))
                    print(f"  · 上传 {path.name}")
                    media_cache[keyword] = upload(path, "image", width, height)
                media_ids.append({"id": media_cache[keyword], "position": 0})

        payload = {
            "title": item["title"],
            "content": item["content"],
            "tags": item["tags"],
            "happenedAt": item["at"],
            "media": media_ids,
        }
        create_moment(payload)
        created += 1
        print(f"  ✓ {item['title']}")

    print(f"\n完成：新增 {created} 段记忆。打开 {API} 看看光河吧。")


if __name__ == "__main__":
    main()
