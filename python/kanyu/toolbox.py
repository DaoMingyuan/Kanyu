"""堪舆工具箱运行时 —— ArcGIS Pro .pyt 式样的 Python 工具箱约定。

一个工具箱是一个 .py 文件，定义一个 Toolbox 子类，内嵌 Tool 子类::

    from kanyu.toolbox import Toolbox, Tool, Param
    import kanyu

    class 规划工具(Toolbox):
        alias = "planning"

        class 缓冲五百(Tool):
            name = "buffer500"
            label = "500 米缓冲"
            description = "对面图层做 500 米缓冲区"
            params = [
                Param("input", "输入图层路径", "path"),
                Param("distance", "缓冲距离", "float", default=500.0),
                Param("out", "输出路径", "path", optional=True),
            ]

            def execute(self, args):
                fc = kanyu.load(args["input"])
                result = kanyu.buffer(fc, float(args["distance"]))
                if args.get("out"):
                    kanyu.export(result, args["out"], args["out"].rsplit(".", 1)[-1])
                    return {"out": args["out"]}
                return result

CLI 驱动（JSON over stdout）：
    kanyu toolbox list mytools.py
    kanyu toolbox run mytools.py buffer500 --param input=a.geojson --param distance=500
"""

from __future__ import annotations

import importlib.util
import json
import sys
from dataclasses import dataclass, field
from typing import Any


@dataclass
class Param:
    """工具参数声明。"""

    name: str
    label: str
    kind: str = "string"  # string | float | int | bool | path
    default: Any = None
    optional: bool = False


class Tool:
    """工具基类：name/label/description/params + execute(args: dict) -> Any。"""

    name: str = ""
    label: str = ""
    description: str = ""
    params: list[Param] = field(default_factory=list)

    def execute(self, args: dict) -> Any:  # pragma: no cover - 由子类实现
        raise NotImplementedError


class Toolbox:
    """工具箱基类：alias + 内嵌 Tool 子类。"""

    alias: str = "toolbox"


def _load_module(path: str):
    spec = importlib.util.spec_from_file_location("kanyu_user_toolbox", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"无法加载工具箱文件: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _collect_tools(module) -> list[tuple[type, type]]:
    """收集工具箱中的工具。

    以"声明模块"判定归属（`__module__` 匹配），而非 `issubclass` 身份比较——
    兼容 `python -m kanyu.toolbox`（模块双实例）与直接 import 两种驱动方式。
    """
    found = []
    for attr in vars(module).values():
        if not isinstance(attr, type):
            continue
        if getattr(attr, "__module__", None) != module.__name__:
            continue  # 排除 import 进来的基类/外部类
        if not isinstance(getattr(attr, "alias", None), str):
            continue  # 工具箱须有 alias
        for inner in vars(attr).values():
            if (
                isinstance(inner, type)
                and getattr(inner, "__module__", None) == module.__name__
                and isinstance(getattr(inner, "name", None), str)
                and callable(getattr(inner, "execute", None))
            ):
                found.append((attr, inner))
    return found


def _tool_manifest(toolbox_cls: type[Toolbox], tool_cls: type[Tool]) -> dict:
    return {
        "toolbox": toolbox_cls.alias,
        "name": tool_cls.name,
        "label": tool_cls.label,
        "description": tool_cls.description,
        "params": [
            {
                "name": p.name,
                "label": p.label,
                "kind": p.kind,
                "default": p.default,
                "optional": p.optional,
            }
            for p in tool_cls.params
        ],
    }


def _emit(obj: Any) -> None:
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))
    sys.stdout.write("\n")
    sys.stdout.flush()


def cmd_list(path: str) -> None:
    module = _load_module(path)
    tools = [_tool_manifest(tb, t) for tb, t in _collect_tools(module)]
    _emit({"toolbox_file": path, "tools": tools})


def cmd_run(path: str, tool_name: str, args: dict) -> None:
    module = _load_module(path)
    for toolbox_cls, tool_cls in _collect_tools(module):
        if tool_cls.name == tool_name:
            # 默认值填充 + 必填校验。
            final_args = {}
            for p in tool_cls.params:
                if p.name in args:
                    final_args[p.name] = args[p.name]
                elif p.default is not None:
                    final_args[p.name] = p.default
                elif not p.optional:
                    raise RuntimeError(f"缺少必填参数: {p.name}（{p.label}）")
            result = tool_cls().execute(final_args)
            _emit({"ok": True, "result": result})
            return
    raise RuntimeError(f"工具不存在: {tool_name}（toolbox list 查看可用工具）")


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        _emit({"ok": False, "error": "用法: python -m kanyu.toolbox list|run <file.py> [tool] [--params-json '{...}']"})
        return 2
    command, path = argv[1], argv[2]
    try:
        if command == "list":
            cmd_list(path)
            return 0
        if command == "run":
            tool_name = argv[3]
            params_json = "{}"
            if "--params-json" in argv:
                params_json = argv[argv.index("--params-json") + 1]
            cmd_run(path, tool_name, json.loads(params_json))
            return 0
        _emit({"ok": False, "error": f"未知命令: {command}"})
        return 2
    except Exception as exc:  # noqa: BLE001 —— 运行时统一错误出口
        _emit({"ok": False, "error": str(exc)})
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
