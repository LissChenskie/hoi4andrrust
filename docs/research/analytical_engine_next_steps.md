# AnalyticalEngine — план следующего этапа аудита

Дата: 2026-10-23  
Область: только CTD-Games/AnalyticalEngine. Репозиторий HOI4 Mobile не использовался как источник кода AnalyticalEngine.

## Уже просмотрено

- pom.xml: Maven-конфигурация с libGDX 1.11.0 и LWJGL3 desktop backend.
- src/AnalyticalEngine/AnalyticalEngine.java: инициализация AnalyticalEngine и ScriptEngine.
- src/AnalyticalEngine/Framework/Javascript.java: запуск JS и связь с Java.
- src/AnalyticalEngine/GameFramework/startup.js, startup_imports.js, global_initialisation.js: последовательность инициализации, Java.type imports и структура глобального состояния.
- src/AnalyticalEngine/GameFramework/core/map/: функции карты, map modes, регионы, ресурсы, terrain.
- src/AnalyticalEngine/GameFramework/core/provinces/: операции над провинциями.
- src/AnalyticalEngine/GameFramework/core/mod/mod_framework.js: загрузка модов связана с SteamManager.modsFolders.
- src/AnalyticalEngine/GameFramework/core/event_handler/event_handler_framework.js: событийные обработчики, включая периодические проверки.
- src/AnalyticalEngine/Logic/Conditions/, Effects/, Scopes/, Parsers/: разнесённые каталоги условий, эффектов, контекстов и парсеров.
- src/AnalyticalEngine/JS/pathfinding_a_star.js: реализация A* для графа.
- src/aoc/kingdoms/lukasz/jakowski/AA_Game.java и src/aoc/kingdoms/lukasz/jakowski/desktop/DesktopLauncher.java: код интеграции AOC3/libGDX и desktop launcher; AA_Game.java помечен как восстановленный декомпилятором файл.

## Следующий этап: рендеринг карты

1. Просмотреть исходники Map.java, Map_Data.java, ProvinceDraw.java, Renderer.java и определить, какие из них действительно представлены исходным Java-кодом, а какие восстановлены из class.
2. Найти формат геометрии провинций и связи между province ID, границами, цветами и hit-testing.
3. Проследить путь ввода: касание/мышь → координаты экрана/карты → выбранная провинция → событие UI.
4. Разобрать map modes: где хранится игровое состояние, где только цвет/визуализация и как запускается перерисовка.
5. Проверить кэширование и обновления: какие данные пересчитываются при изменении владельца/режима карты.
6. Сформулировать переносимые принципы для независимого Rust-кода; не переносить код, игровые ресурсы или карту AOC3 как будто это данные HOI4.

## Ограничения

Пока не проводились локальная сборка, запуск игры, измерения производительности или полный построчный аудит. Наличие каталогов/функций не доказывает полноту реализации. Необходимо отдельно проверять права на бинарники, декомпилированный код и ресурсы.
