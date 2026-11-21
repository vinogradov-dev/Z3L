//! The table: every user-facing string, one row per message.
//!
//! A row is `Variant = [English, Russian]`. The variant is the key, the pair is the translation, and
//! the array length is [`Lang::COUNT`], so adding a language breaks every row until it is filled in.
//! Rows are grouped by the surface that draws them; the group comment is a reading aid, not data.

use crate::Lang;

messages! {
    // ── The table's own tests ───────────────────────────────────────────────
    TestPair = ["{} and {}", "{} и {}"],
    TestPointOne = ["{} point", "{} точка"],
    TestPointFew = ["{} points", "{} точки"],
    TestPointMany = ["{} points", "{} точек"],









    // ── Header and file menu ────────────────────────────────────────────────
    HeaderHome = ["Home", "Главная"],
    HeaderSave = ["Save", "Сохранить"],
    HeaderUndo = ["Undo", "Отменить"],
    HeaderNothingToUndo = ["Nothing to undo", "Нечего отменять"],
    HeaderRedo = ["Redo", "Повторить"],
    HeaderNothingToRedo = ["Nothing to redo", "Нечего повторять"],
    HeaderNewScene = ["New scene", "Новая сцена"],
    HeaderUnsaved = ["Unsaved changes", "Есть несохранённые изменения"],
    HeaderCloseTab = ["Close the tab", "Закрыть вкладку"],
    CollapseAllPanels = ["Collapse all panels", "Свернуть все панели"],
    ExpandAllPanels = ["Expand all panels", "Развернуть все панели"],
    HideAllPanels = ["Hide all panels", "Скрыть все панели"],
    ShowAllPanels = ["Show all panels", "Показать все панели"],
    FoldRibbon = ["Fold the ribbon", "Свернуть ленту"],
    UnfoldRibbon = ["Unfold the ribbon", "Развернуть ленту"],
    ExpandPanel = ["Expand panel", "Развернуть панель"],
    File = ["File", "Файл"],
    MenuNew = ["New", "Новый"],
    MenuOpen = ["Open…", "Открыть…"],
    MenuSave = ["Save", "Сохранить"],
    MenuSaveAs = ["Save As…", "Сохранить как…"],
    MenuCloseTab = ["Close tab", "Закрыть вкладку"],
    MenuExit = ["Exit", "Выход"],
    Untitled = ["Untitled", "Без имени"],
    CloseTitle = ["Close “{}”?", "Закрыть «{}»?"],
    CloseBody = [
        "This scene has unsaved changes. Closing the tab discards them.",
        "В сцене есть несохранённые изменения. Закрытие вкладки отбросит их."
    ],
    DiscardAndClose = ["Discard and close", "Отбросить и закрыть"],
    QuitOneBody = [
        "One open scene has unsaved changes. Quitting discards them.",
        "В одной открытой сцене есть несохранённые изменения. Выход отбросит их."
    ],
    QuitManyOne = ["{} open scene has unsaved changes. Quitting discards them.", "{} открытая сцена содержит несохранённые изменения. Выход отбросит их."],
    QuitManyFew = ["{} open scenes have unsaved changes. Quitting discards them.", "{} открытые сцены содержат несохранённые изменения. Выход отбросит их."],
    QuitManyMany = ["{} open scenes have unsaved changes. Quitting discards them.", "{} открытых сцен содержат несохранённые изменения. Выход отбросит их."],
    QuitTitle = ["Quit {}?", "Выйти из {}?"],
    DiscardAndQuit = ["Discard and quit", "Отбросить и выйти"],
    DeleteNodesTitleOne = ["Delete {} object?", "Удалить {} объект?"],
    DeleteNodesTitleFew = ["Delete {} objects?", "Удалить {} объекта?"],
    DeleteNodesTitleMany = ["Delete {} objects?", "Удалить {} объектов?"],
    DeleteNodesBody = [
        "The whole selection is removed in one step. Undo (Ctrl+Z) brings it back.",
        "Всё выделение удаляется за один шаг. Отмена (Ctrl+Z) вернёт его."
    ],
    OpenSceneTitle = ["Open scene", "Открыть сцену"],
    SaveImageAsTitle = ["Save the image as", "Сохранить изображение как"],
    ImageFiles = ["PNG images", "Изображения PNG"],
    SaveSceneAsTitle = ["Save scene as", "Сохранить сцену как"],
    SceneFiles = ["Scene files", "Сцены"],
    PickFailed = [
        "The system file manager could not be opened.",
        "Не удалось открыть системный файловый менеджер."
    ],
    NoticeNoSettingsLocation = [
        "No workspace settings location; using defaults.",
        "Нет расположения настроек рабочей области; используются значения по умолчанию."
    ],
    NoticeNoSavedSettings = [
        "No saved workspace settings yet; using the defaults.",
        "Сохранённых настроек ещё нет; используются значения по умолчанию."
    ],
    NoticeReadFailed = [
        "Could not read the workspace settings ({}); using defaults.",
        "Не удалось прочитать настройки рабочей области ({}); используются значения по умолчанию."
    ],
    NoticeUnusable = [
        "Could not use the workspace settings ({}); using defaults.",
        "Не удалось применить настройки рабочей области ({}); используются значения по умолчанию."
    ],
    NoticeUnknownPalette = [
        "The saved palette “{}” is unknown to this build; {} is in use.",
        "Сохранённая палитра «{}» неизвестна этой сборке; используется {}."
    ],

    // ── Status bar ──────────────────────────────────────────────────────────
    LockedStatus = ["Locked · {}", "Заблокировано · {}"],
    StatusSelectionCount = ["Selected: {}", "Выбрано: {}"],
    StatusSize = ["{} × {} × {}", "{} × {} × {}"],
    StatusBanding = ["edge {}", "кромка {}"],
    StatusReady = ["Ready", "Готово"],
    StatusRebuilding = ["Rebuilding…", "Перестроение…"],
    StatusScale = ["{} %", "{} %"],
    UnlockHint = [
        "Unlock it in the scene tree to move or delete it",
        "Снимите блокировку в дереве сцены, чтобы переместить или удалить"
    ],
    AddingTo = ["Adding to: {}", "Добавление в: {}"],
    CreateAtRoot = ["Create at the scene root again", "Снова создавать в корне сцены"],
    CreateAtRootHint = [
        "The next created node lands at the scene root",
        "Следующий созданный узел попадёт в корень сцены"
    ],
    ContourEdit = ["Contour edit", "Редактирование контура"],
    ContourEditStatusOne = ["Contour edit · {} segment", "Редактирование контура · {} сегмент"],
    ContourEditStatusFew = ["Contour edit · {} segments", "Редактирование контура · {} сегмента"],
    ContourEditStatusMany = ["Contour edit · {} segments", "Редактирование контура · {} сегментов"],
    ContourModifiedSuffix = [" · modified", " · изменён"],
    AlongContour = ["Along contour", "Вдоль контура"],
    FreeMove = ["Free move", "Свободное перемещение"],
    ContourAlongHint = [
        "Drag corners along their segment, or click a corner twice to move it  ·  double-click a side to split  ·  Enter apply  ·  Esc cancel",
        "Перетаскивайте углы вдоль их сегмента или щёлкните угол дважды, чтобы перенести  ·  двойной щелчок по стороне делит её  ·  Enter применить  ·  Esc отмена"
    ],
    ContourFreeHint = [
        "Drag corners freely, or click a corner twice to move it  ·  double-click a side to split  ·  Enter apply  ·  Esc cancel",
        "Перетаскивайте углы свободно или щёлкните угол дважды, чтобы перенести  ·  двойной щелчок по стороне делит её  ·  Enter применить  ·  Esc отмена"
    ],
    DrawStatus = ["Draw · {}{}{}", "Рисование · {}{}{}"],
    PointsSuffixOne = [" · {} point", " · {} точка"],
    PointsSuffixFew = [" · {} points", " · {} точки"],
    PointsSuffixMany = [" · {} points", " · {} точек"],
    DrawArcSuffix = [" · arc", " · дуга"],
    DrawLineSuffix = [" · line", " · линия"],
    SnapTenMillimetres = ["Snap 10 mm  (G)", "Привязка 10 мм  (G)"],
    FinishEnter = ["Finish  (Enter)", "Завершить  (Enter)"],
    ApplyEnter = ["Apply  (Enter)", "Применить  (Enter)"],
    CancelEsc = ["Cancel  (Esc)", "Отмена  (Esc)"],
    DrawHint = [
        "LMB add point  ·  Tab next field  ·  Enter places the point  ·  RMB line/arc  ·  Shift axis lock  ·  Alt ignores snaps  ·  Esc cancel",
        "ЛКМ добавить точку  ·  Tab следующее поле  ·  Enter ставит точку  ·  ПКМ линия/дуга  ·  Shift фиксация оси  ·  Alt игнорирует привязки  ·  Esc отмена"
    ],
    MeasureFirstPoint = [
        "Measure · click the first point  ·  Esc leaves",
        "Измерение · щёлкните первую точку  ·  Esc выходит"
    ],
    MeasureSecondPoint = [
        "Measure · click the second point  ·  Esc leaves",
        "Измерение · щёлкните вторую точку  ·  Esc выходит"
    ],
    MeasureAgain = [
        "Measure · click for a new measurement  ·  Esc leaves",
        "Измерение · щёлкните для нового измерения  ·  Esc выходит"
    ],
    MeasureValue = ["Length {}", "Длина {}"],
    VoidHint = [
        "Niche · point at the empty space  ·  Esc leaves",
        "Ниша · укажите на пустое место  ·  Esc выходит"
    ],
    VoidSize = ["Niche {}", "Ниша {}"],
    VoidOutside = [
        "Point at the empty space inside the open product",
        "Укажите на пустое место внутри открытого изделия"
    ],
    VoidUnrepresentable = [
        "The empty space here is not one straight shape; point at a plainer gap",
        "Пустое место здесь не одна прямая форма; укажите на более простой зазор"
    ],
    VoidFootprint = [
        "The open piece's frame is not convex; point at a plainer gap",
        "Контур открытого изделия невыпуклый; укажите на более простой зазор"
    ],
    NavigationStatus = ["Navigation · {}", "Навигация · {}"],
    GestureOrbit = ["Drag in the viewport to orbit  ·  Esc done", "Перетаскивайте в области просмотра для облёта  ·  Esc готово"],
    GesturePan = ["Drag in the viewport to pan  ·  Esc done", "Перетаскивайте в области просмотра для панорамы  ·  Esc готово"],
    GestureZoom = ["Drag vertically to zoom  ·  Esc done", "Перетаскивайте вертикально для масштаба  ·  Esc готово"],
    NavOrbitLabel = ["Orbit  ·  drag in the viewport", "Облёт  ·  перетаскивание в области просмотра"],
    NavPanLabel = ["Pan  ·  drag in the viewport", "Панорама  ·  перетаскивание в области просмотра"],
    NavZoomLabel = ["Zoom  ·  drag vertically", "Масштаб  ·  перетаскивание по вертикали"],
    Gizmo = ["Gizmo", "Гизмо"],
    RollToDegree = ["Roll to {}°", "Крен до {}°"],
    HomeHint = ["Home  ·  back to the axonometric view", "Домой  ·  вернуться к аксонометрии"],
    ProjectionOrthoShort = ["Ortho", "Орто"],
    ProjectionPerspShort = ["Persp", "Персп"],
    OrientationReadout = ["Pitch {} · Yaw {} · Roll {} · {}", "Тангаж {} · Рыскание {} · Крен {} · {}"],
    Move = ["Move", "Перемещение"],
    Select = ["Select", "Выбор"],
    Rotate = ["Rotate", "Вращение"],
    Object = ["Object", "Объект"],
    SelectHint = ["Select: a click picks, a drag draws a band", "Выбор: щелчок выбирает, перетаскивание рисует рамку"],
    MoveHint = [
        "Move freely: the pointer's own distance, no snapping",
        "Двигать свободно: расстояние по указателю, без привязок"
    ],
    ObjectHint = [
        "Move a body: click to take it, move the pointer, click to put it down; it stops at another body",
        "Двигать тело: щелчок берёт его, укажите новое место, щелчок ставит; тело упирается в другое"
    ],
    ObjectHold = [
        "Piece in hand: the pointer moves it, a click puts it down, Esc puts it back",
        "Деталь в руке: указатель её двигает, щелчок ставит, Esc возвращает"
    ],
    RotateHint = ["Turn the selection about its pivot", "Поворачивать выделение вокруг пивота"],
    ObjectRotate = ["Turn", "Поворот"],
    ObjectRotateHint = [
        "Turn a body about its pivot: the ring stops where another body stops it",
        "Поворачивать тело вокруг пивота: кольцо останавливается там, где его останавливает другое тело"
    ],
    MeshEditStatusOne = ["Mesh edit · {} vertex selected", "Редактирование сетки · выбрана {} вершина"],
    MeshEditStatusFew = ["Mesh edit · {} vertices selected", "Редактирование сетки · выбрано {} вершины"],
    MeshEditStatusMany = ["Mesh edit · {} vertices selected", "Редактирование сетки · выбрано {} вершин"],
    MeshFaceStatusOne = ["Mesh edit · {} face selected", "Редактирование сетки · выбрана {} грань"],
    MeshFaceStatusFew = ["Mesh edit · {} faces selected", "Редактирование сетки · выбрано {} грани"],
    MeshFaceStatusMany = ["Mesh edit · {} faces selected", "Редактирование сетки · выбрано {} граней"],
    MeshEditHint = [
        "LMB select  ·  Shift/Ctrl add  ·  Esc done  ·  MMB orbit",
        "ЛКМ выбрать  ·  Shift/Ctrl добавить  ·  Esc готово  ·  СКМ облёт"
    ],
    ViewportHint = [
        "LMB select  ·  double-click enter  ·  Esc leave  ·  MMB orbit  ·  Shift+MMB pan  ·  Ctrl+MMB zoom",
        "ЛКМ выбрать  ·  двойной щелчок войти  ·  Esc выйти  ·  СКМ облёт  ·  Shift+СКМ панорама  ·  Ctrl+СКМ масштаб"
    ],

    // ── Side panels and viewport cluster ────────────────────────────────────
    PanelScene = ["Scene", "Сцена"],
    PanelInspector = ["Inspector", "Инспектор"],
    FoldPanel = ["Fold panel", "Свернуть панель"],
    OpenPanel = ["Open the {} panel", "Открыть панель «{}»"],
    Layout = ["Layout", "Раскладка"],
    LayoutOneView = ["One view", "Один вид"],
    LayoutTwoVertical = ["Two vertical", "Два по вертикали"],
    LayoutTwoHorizontal = ["Two horizontal", "Два по горизонтали"],
    LayoutFourViews = ["Four views", "Четыре вида"],
    ViewsOfMax = ["{} of {} views · drag a border to resize", "{} из {} видов · перетащите границу, чтобы изменить размер"],
    Restore = ["Restore", "Восстановить"],
    Expand = ["Expand", "Развернуть"],
    DisplaySettings = ["Display settings", "Настройки отображения"],
    ViewPreset = ["View preset", "Предустановка вида"],
    ShadingGroup = ["Shading", "Заливка"],
    ShadingGroupHint = [
        "How this view draws the model: one of the five shading styles, and the filled cell is the one it uses",
        "Как этот вид рисует модель: один из пяти стилей заливки, заполненная ячейка — используемый"
    ],
    DisplayHeading = ["Display", "Отображение"],
    ShowOnScreenDisplay = ["Show the on-screen display", "Показывать экранную информацию"],
    OnScreenDisplayOn = [
        "The corner block names the angles, the projection, the frame rate and the scene's cost",
        "Блок в углу показывает углы, проекцию, частоту кадров и стоимость сцены"
    ],
    OnScreenDisplayOff = [
        "Draw the angles, the projection, the frame rate and the scene's cost in the free corner",
        "Рисовать углы, проекцию, частоту кадров и стоимость сцены в свободном углу"
    ],
    OsdRate = ["{} fps · {} ms", "{} кадр/с · {} мс"],
    OsdScene = ["{} objects · {} triangles", "{} объектов · {} треугольников"],
    OsdRebuild = ["rebuild {} ms", "перестроение {} мс"],
    OsdUpload = ["upload {} ms", "выгрузка {} мс"],
    OsdPick = ["pick {} ms", "выбор {} мс"],
    ShowOrientationWidget = ["Show the orientation cube", "Показывать куб ориентации"],
    OrientationWidgetOn = [
        "The corner cube, its home button and the projection toggle are drawn beside the panels",
        "Куб в углу, кнопка «домой» и переключатель проекции рисуются рядом с панелями"
    ],
    OrientationWidgetOff = [
        "Hide the corner cube, its home button and the projection toggle; the views stay reachable from the cluster",
        "Скрыть куб в углу, кнопку «домой» и переключатель проекции; виды остаются доступны из кластера"
    ],
    AuthoredCameraOwns = ["An authored camera owns the projection", "Проекцию задаёт авторская камера"],
    GridStrength = ["Floor grid", "Сетка пола"],
    EdgesStrength = ["Outlines", "Контуры"],
    ShadowStrength = ["Shadow strength", "Сила тени"],
    GridStrengthHint = [
        "Brightness of the work-plane floor grid; 0 hides it.",
        "Яркость сетки пола; 0 скрывает её."
    ],
    EdgesStrengthHint = [
        "Strength of the form-outline pass; 0 leaves the fill only.",
        "Сила контурной обводки; 0 оставляет только заливку."
    ],
    ShadowStrengthHint = [
        "How far the floor darkens under the shadow the key light casts.",
        "Насколько сильно тень от ключевого света затемняет пол."
    ],
    FloorOpacity = ["Floor opacity", "Непрозрачность пола"],
    FloorOpacityHint = [
        "How solid the floor plate is; below 100 you see what stands under it through the tint, and 0 hides the plate and its shadow.",
        "Насколько плотна плита пола; ниже 100 сквозь неё видно то, что стоит под ней, а 0 скрывает плиту и её тень."
    ],
    DisplayConfigure = ["Configure…", "Настроить…"],
    DisplayConfigureHint = [
        "Opens Settings › View for the display strengths and the floor's opacity.",
        "Открывает «Настройки › Вид» для силы отображения и непрозрачности пола."
    ],
    ResetDisplay = ["Reset display", "Сбросить отображение"],
    ResetDisplayHint = [
        "Restores the floor grid, outline, ground shadow and floor opacity to 60 / 70 / 40 / 100.",
        "Возвращает силу сетки пола, контуров, тени и непрозрачность пола: 60 / 70 / 40 / 100."
    ],
    SectionAdditional = ["Additional", "Дополнительно"],
    WorkPlane = ["Work plane · {}", "Рабочая плоскость · {}"],
    PerspectiveViewHint = [
        "Perspective view; switch to orthographic  (O)",
        "Перспективный вид; переключить на ортогональный  (O)"
    ],
    OrthographicViewHint = [
        "Orthographic view; switch to perspective  (O)",
        "Ортогональный вид; переключить на перспективный  (O)"
    ],

    // ── Machining ───────────────────────────────────────────────────────────
    NoMachining = ["No machining.", "Обработок нет."],
    MachiningClassNotAdmitted = [
        "The class does not allow this machining.",
        "Класс не допускает эту обработку."
    ],
    MachiningCutout = ["Cutout", "Вырез"],
    MachiningCategoryCut = ["Cut", "Вырезание"],
    ShowHideOperation = ["Show or hide this operation", "Показать или скрыть операцию"],
    MoveEarlier = ["Move earlier", "Переместить раньше"],
    MoveLater = ["Move later", "Переместить позже"],
    RemoveOperation = ["Remove operation", "Удалить операцию"],
    AddOperation = ["Add operation", "Добавить операцию"],
    FaceWith = ["Face {}", "Грань {}"],
    EdgeWith = ["Edge {}", "Кромка {}"],
    AnchorWith = ["Anchor  {}", "Привязка: {}"],
    Entry = ["Entry", "Вход"],
    Through = ["Through", "Насквозь"],
    Blind = ["Blind", "Глухой"],
    CutoutReshapeHint = [
        "Reshapes this ring in the viewport. Undo restores the stored ring.",
        "Изменяет это кольцо в области просмотра. Отмена вернёт сохранённое кольцо."
    ],
    ArcSuffixOne = [" · {} arc", " · {} дуга"],
    ArcSuffixFew = [" · {} arcs", " · {} дуги"],
    ArcSuffixMany = [" · {} arcs", " · {} дуг"],
    Depth = ["Depth", "Глубина"],
    NotALength = ["Not a length: {}", "Не длина: {}"],
    NotAnAngle = ["Not an angle: {}", "Не угол: {}"],

    // ── Object card ─────────────────────────────────────────────────────────
    ObjectNameEmpty = ["Object name must not be empty.", "Имя объекта не должно быть пустым."],
    ObjectSelectSingle = [
        "Select only this object to edit it or to collapse it.",
        "Выберите только этот объект, чтобы его редактировать или свернуть."
    ],
    VerticesCount = ["Vertices {}", "Вершин: {}"],
    FacesCount = ["Faces {}", "Граней: {}"],
    PrimitiveSourceTruth = [
        "The parameters are the source of truth; the mesh is derived.",
        "Параметры — источник истины; сетка выводится из них."
    ],
    ContourSegmentsCount = ["Contour segments {}", "Сегментов контура: {}"],
    HeightRow = ["Height {}", "Высота: {}"],
    ContourSourceTruth = [
        "The contour and height are the source of truth; the mesh is derived.",
        "Контур и высота — источник истины; сетка выводится из них."
    ],
    PathSegmentsCount = ["Path segments {}", "Сегментов пути: {}"],
    ClosedChain = ["Closed chain", "Замкнутая цепь"],
    OpenChain = ["Open chain", "Разомкнутая цепь"],
    PathSourceTruth = ["The drawing is the source of truth; it has no mesh yet.", "Чертёж — источник истины; сетки пока нет."],
    MeshSourceTruth = ["Editable as a mesh; undo is its only history.", "Редактируется как сетка; история — только отмена."],
    FieldRadius = ["Radius", "Радиус"],
    FieldTopRadius = ["Top radius", "Верхний радиус"],
    FieldWidth = ["Width", "Ширина"],
    FieldEndAngle = ["End angle", "Конечный угол"],
    PrimitiveRefused = ["The primitive was refused: {}", "Примитив отклонён: {}"],
    FigureRefused = ["The figure was refused: {}", "Фигура отклонена: {}"],
    FieldDepth = ["Depth", "Глубина"],
    FieldBands = ["Bands", "Полосы"],
    EditMesh = ["Edit mesh", "Редактировать сетку"],
    VerticesPlural = ["Vertices", "Вершины"],
    FacesPlural = ["Faces", "Грани"],
    SelectedVerticesCount = ["Selected vertices  {}", "Выбрано вершин: {}"],
    MoreCountOne = ["… and {} more", "… и ещё {}"],
    MoreCountFew = ["… and {} more", "… и ещё {}"],
    MoreCountMany = ["… and {} more", "… и ещё {}"],
    ClickVertexHint = [
        "Click a vertex in the viewport; Shift or Ctrl adds it to the selection.",
        "Щёлкните вершину в области просмотра; Shift или Ctrl добавит её к выделению."
    ],
    SelectedFacesCount = ["Selected faces  {}", "Выбрано граней: {}"],
    ClickFaceHint = [
        "Click a face in the viewport; Shift or Ctrl adds it to the selection.",
        "Щёлкните грань в области просмотра; Shift или Ctrl добавит её к выделению."
    ],
    ObjectContourEditingInViewport = [
        "Editing the contour in the viewport. Apply or cancel in the viewport toolbar.",
        "Контур редактируется в области просмотра. Примените или отмените на панели инструментов области просмотра."
    ],
    DragCornersHint = ["Drag corners in the viewport; double-click a side to split it.", "Перетаскивайте углы в области просмотра; двойной щелчок по стороне делит её."],
    ContourHasArcs = [
        "This contour has arcs; corners cannot describe it, so the text editor is off.",
        "В этом контуре есть дуги; углы не описывают его, поэтому текстовый редактор отключён."
    ],
    ContourTextHint = ["{} points; one per line as `X, Y` in millimetres.", "{} точек; по одной в строке в виде `X, Y` в миллиметрах."],
    ApplyContour = ["Apply contour", "Применить контур"],
    CollapseWarning = [
        "Baking the parameters into a mesh deletes them as parameters. Undo is the only way back.",
        "Превращение параметров в сетку удаляет их как параметры. Вернуть можно только отменой."
    ],
    CollapseToMesh = ["Collapse to mesh", "Свернуть в сетку"],
    CollapseToMeshEllipsis = ["Collapse to mesh…", "Свернуть в сетку…"],
    LineExpectedPoint = ["Line {}: expected `X, Y` in millimetres.", "Строка {}: ожидается `X, Y` в миллиметрах."],
    LineWithError = ["Line {}: {}.", "Строка {}: {}."],
    PrimitiveBadgeMesh = ["Mesh", "Сетка"],
    PrimitiveBadgePrimitive = ["Primitive", "Примитив"],
    PrimitiveBadgeConstructive = ["Constructive", "Конструктив"],
    PrimitiveBadgePath = ["Path", "Путь"],
    PrimitiveBadgeContours = ["Contours", "Контуры"],

    // ── Panel card ──────────────────────────────────────────────────────────
    PanelEditingInViewport = [
        "Editing in the viewport. Apply or cancel in the viewport toolbar.",
        "Редактирование в области просмотра. Примените или отмените на панели инструментов области просмотра."
    ],
    PanelFinishContour = [
        "Finish the current contour edit in the viewport first.",
        "Сначала завершите текущее редактирование контура в области просмотра."
    ],
    PanelSelectSingle = [
        "Select only this panel to edit its contour.",
        "Выберите только эту панель, чтобы редактировать её контур."
    ],
    PanelEditContour = ["Edit contour", "Редактировать контур"],
    CornerNone = ["None", "Нет"],
    CornerRadius = ["Radius", "Радиус"],
    CornerChamfer = ["Chamfer", "Фаска"],
    PanelNameEmpty = ["Panel name must not be empty.", "Имя панели не должно быть пустым."],
    SectionDimensions = ["Dimensions", "Размеры"],
    SectionNominalSize = ["Nominal size", "Номинальный размер"],
    PanelContourSetsSize = [
        "The contour ring sets the length and width.",
        "Длину и ширину задаёт контурное кольцо."
    ],
    PanelClassLengthOnly = [
        "The class fixes the width: this object resizes by length only.",
        "Класс фиксирует ширину: этот объект меняется только по длине."
    ],
    PanelClassSizeFixed = [
        "The class fixes the size: this object is not resized.",
        "Класс фиксирует размер: этот объект не изменяется."
    ],
    Length = ["Length", "Длина"],
    Width = ["Width", "Ширина"],
    LengthField = ["Length: {}", "Длина: {}"],
    WidthField = ["Width: {}", "Ширина: {}"],
    SectionResolved = ["Resolved", "Расчётные"],
    SectionMaterial = ["Material", "Материал"],
    SectionShape = ["Shape", "Форма"],
    SectionBanding = ["Banding", "Кромка"],
    PanelClassNoBanding = ["The class does not allow edge banding.", "Класс не допускает кромление."],
    PanelClassNoContour = ["The class does not allow outline edits.", "Класс не допускает редактирование контура."],
    SectionMachining = ["Machining", "Обработка"],
    SectionDiagnostics = ["Diagnostics", "Диагностика"],
    PanelMaterialMissingCatalogue = [
        "Panel material {} is missing from the catalogue.",
        "Материал панели {} отсутствует в каталоге."
    ],
    MissingMaterial = ["Missing material {}", "Материал {} отсутствует"],
    NoMaterial = ["No material", "Нет материала"],
    SectionOutline = ["Outline", "Контур"],
    SectionFootprint = ["Base outline", "Контур основания"],
    ProductFrameNeedsHeight = ["A frame's outline is finished by its height pull.", "Контур каркаса завершается вытягиванием высоты."],
    ProductContourDraw = ["Draw the {} base outline on the plane, then pull its height.", "Нарисуйте контур основания {} на плоскости, затем вытяните высоту."],
    PanelContourRingOne = ["Contour ring · {} segment", "Контурное кольцо · {} сегмент"],
    PanelContourRingFew = ["Contour ring · {} segments", "Контурное кольцо · {} сегмента"],
    PanelContourRingMany = ["Contour ring · {} segments", "Контурное кольцо · {} сегментов"],
    ArcsCount = ["Arcs {}", "Дуги: {}"],
    BakeOutlineHint = [
        "Bakes the outline into a ring. Corners, chamfers and bows become segments; undo restores them.",
        "Преобразует контур в кольцо. Углы, фаски и прогибы становятся сегментами; отмена вернёт их."
    ],
    ContourReshapeHint = [
        "Reshapes the ring in the viewport. Undo restores it.",
        "Изменяет кольцо в области просмотра. Отмена вернёт прежнее."
    ],
    SectionBowedEdges = ["Bowed edges", "Изогнутые кромки"],
    SectionBevels = ["End bevels", "Скосы торцов"],
    NoBevelledEnds = ["This outline has no straight edge to bevel.", "В этом контуре нет прямой кромки для скоса."],
    BevelAngleHint = [
        "The end face tilts by this angle across the thickness; 0 to 89°.",
        "Торец наклоняется на этот угол по толщине; от 0 до 89°."
    ],
    CornerWith = ["Corner {}", "Угол {}"],
    Inward = ["in", "внутрь"],
    Outward = ["out", "наружу"],
    Clear = ["Clear", "Очистить"],
    NoBanding = ["No banding.", "Кромок нет."],
    AddChamferOrRadius = [
        "Add a chamfer or radius to band a segment.",
        "Добавьте фаску или радиус, чтобы оклеить сегмент."
    ],
    AllSegmentsBanded = ["All existing segments are banded.", "Все существующие сегменты оклеены."],
    AddEdgeMaterial = [
        "Add an edge material to the catalogue first.",
        "Сначала добавьте материал кромки в каталог."
    ],
    AddBanding = ["Add banding", "Добавить кромку"],
    ChooseSegment = ["Choose a segment", "Выберите сегмент"],
    MachiningIssueOne = ["1 machining issue is shown with its operation.", "1 замечание по обработке показано у операции."],
    MachiningIssueFew = ["{} machining issues are shown with their operations.", "{} замечания по обработке показаны у операций."],
    MachiningIssueMany = ["{} machining issues are shown with their operations.", "{} замечаний по обработке показаны у операций."],
    NoIssues = ["No issues.", "Замечаний нет."],
    ThicknessRow = ["Thickness  {}", "Толщина: {}"],
    CoreRow = ["Core  {} × {}", "Сердцевина: {} × {}"],
    OverallRow = ["Overall  {} × {}", "Габарит: {} × {}"],
    GeometryError = ["Geometry: {}", "Геометрия: {}"],
    EdgeBottom = ["Bottom", "Нижняя"],
    EdgeRight = ["Right", "Правая"],
    EdgeTop = ["Top", "Верхняя"],
    EdgeLeft = ["Left", "Левая"],
    EdgeGeneric = ["Edge", "Кромка"],
    DiagNoMaterial = ["No panel material is assigned.", "Материал панели не назначен."],
    DiagPanelMaterialMissing = [
        "Panel material {} is missing from the catalogue.",
        "Материал панели {} отсутствует в каталоге."
    ],
    DiagEdgeMaterialMissing = [
        "Edge {} uses missing edge material {}.",
        "Кромка {} использует отсутствующий материал {}."
    ],
    DiagCornerMaterialMissing = [
        "Corner {} uses missing edge material {}.",
        "Угол {} использует отсутствующий материал {}."
    ],
    DiagSegmentMaterialMissing = [
        "Segment {} uses missing edge material {}.",
        "Сегмент {} использует отсутствующий материал {}."
    ],
    DiagNonPositiveCore = ["The panel core is not positive.", "Сердцевина панели не положительна."],
    DiagInvalidOutline = [
        "The panel outline or its edge banding does not form a valid contour.",
        "Контур панели или его кромка не образуют допустимый контур."
    ],
    DiagMachiningOutsideCore = ["Machining {} lies outside the core.", "Обработка {} выходит за сердцевину."],
    DiagCoreRemoved = ["The cutouts remove the whole panel core.", "Вырезы удаляют всю сердцевину панели."],
    DiagCoreDisconnected = [
        "The cutouts split the panel core into disconnected pieces.",
        "Вырезы разделяют сердцевину панели на несвязанные части."
    ],
    DiagBlindDepth = [
        "Machining {} blind depth {} is not less than thickness {}.",
        "Глубина {} обработки {} не меньше толщины {}."
    ],
    SegmentWithArc = ["Segment {} (arc)", "Сегмент {} (дуга)"],
    SegmentWith = ["Segment {}", "Сегмент {}"],
    CornerGeneric = ["Corner", "Угол"],
    CornerRadiusSuffix = [" (radius)", " (радиус)"],
    CornerChamferSuffix = [" (chamfer)", " (фаска)"],
    CornerWithShape = ["Corner {}{}", "Угол {}{}"],
    Included = ["Included", "Включено"],
    RemoveBanding = ["Remove banding", "Убрать кромку"],

    // ── Camera and light cards ──────────────────────────────────────────────
    SectionProjection = ["Projection", "Проекция"],
    Perspective = ["Perspective", "Перспективная"],
    Orthographic = ["Orthographic", "Ортогональная"],
    FieldOfView = ["Field of view", "Поле зрения"],
    BetweenRange = ["Between {} and {}.", "От {} до {}."],
    CameraHeightHint = ["2000 mm", "2000 мм"],
    PositiveAtMost = ["Positive and at most {}.", "Положительное и не больше {}."],
    SectionRenderView = ["Render view", "Вид рендера"],
    RenderFromCamera = ["Render the scene from this camera", "Рендерить сцену с этой камеры"],
    CameraNameEmpty = ["Camera name must not be empty.", "Имя камеры не должно быть пустым."],
    FieldOfViewError = ["Field of view: {}.", "Поле зрения: {}."],
    FovMustBeBetween = ["Field of view must be between {} and {}.", "Поле зрения должно быть от {} до {}."],
    OrthoHeightError = ["Orthographic height: {}.", "Ортогональная высота: {}."],
    OrthoHeightPositiveAtMost = [
        "Orthographic height must be positive and at most {}.",
        "Ортогональная высота должна быть положительной и не больше {}."
    ],
    LightNameEmpty = ["Light name must not be empty.", "Имя источника света не должно быть пустым."],
    SectionLight = ["Light", "Свет"],
    Sun = ["Sun", "Солнце"],
    Point = ["Point", "Точечный"],
    Colour = ["Colour", "Цвет"],
    PowerWatts = ["Power (W)", "Мощность (Вт)"],
    Intensity = ["Intensity", "Интенсивность"],
    Power = ["Power", "Мощность"],
    ColourComponents = ["Colour components must be between 0 and 1.", "Компоненты цвета должны быть от 0 до 1."],
    IntensityRange = ["{} must be between 0 and {}.", "{}: допустимо от 0 до {}."],


    // ── Semantic classes (ADR 0094) ──────────────────────────────────────────
    ClassFamilySeating = ["Seating", "Места для сидения"],
    ClassFamilyLying = ["Lying", "Места для лежания"],
    ClassFamilyStorage = ["Storage", "Хранение"],
    ClassFamilySurfaces = ["Surfaces", "Поверхности"],
    ClassFamilyFittings = ["Fittings", "Фурнитура"],
    ClassFamilyAppliances = ["Appliances", "Бытовая техника"],
    ClassFamilyBoardsTrim = ["Boards and trim", "Плиты и погонаж"],
    ClassFamilyGeneral = ["General", "Общее"],

    ClassChair = ["Chair", "Стул"],
    ClassStool = ["Stool", "Табурет"],
    ClassBench = ["Bench", "Скамья"],
    ClassSofa = ["Sofa", "Диван"],
    ClassBed = ["Bed", "Кровать"],
    ClassMattress = ["Mattress", "Матрас"],
    ClassCabinet = ["Cabinet", "Шкаф корпусный"],
    ClassBaseCabinet = ["Base cabinet", "Шкаф нижний"],
    ClassWallCabinet = ["Wall cabinet", "Шкаф навесной"],
    ClassTallCabinet = ["Tall cabinet", "Шкаф высокий"],
    ClassVerticalPanel = ["Upright", "Стойка"],
    ClassHorizontalPanel = ["Horizontal", "Горизонт"],
    ClassBackPanel = ["Back", "Стенка"],
    ClassDrawerBoxPart = ["Drawer box part", "Деталь ящика"],
    ClassGlassShelf = ["Glass shelf", "Стеклянная полка"],
    ClassTamburatPanel = ["Tamburat panel", "Плита тамбурат"],
    ClassBentPanel = ["Bent panel", "Гнутая панель"],
    ClassHingedDoorMade = ["Hinged door, made", "Дверь распашная, своя"],
    ClassHingedDoorBought = ["Hinged door, bought", "Дверь распашная, покупная"],
    ClassSlidingDoorMade = ["Sliding door, made", "Дверь раздвижная, своя"],
    ClassSlidingDoorBought = ["Sliding door, bought", "Дверь раздвижная, покупная"],
    ClassLiftUpMade = ["Lift-up front, made", "Откидной фасад, свой"],
    ClassLiftUpBought = ["Lift-up front, bought", "Откидной фасад, покупной"],
    ClassDrawerFrontMade = ["Drawer front, made", "Фасад ящика, свой"],
    ClassDrawerFrontBought = ["Drawer front, bought", "Фасад ящика, покупной"],
    ClassDrawerMade = ["Drawer, made", "Ящик, свой"],
    ClassDrawerBought = ["Drawer, bought", "Ящик, покупной"],
    ClassTable = ["Table", "Стол"],
    ClassFloorTable = ["Floor table", "Стол напольный"],
    ClassWallTable = ["Wall table", "Стол навесной"],
    ClassCardHinge = ["Card hinge", "Петля карточная"],
    ClassCardHingeOverlay = ["Card hinge, overlay", "накладная"],
    ClassCardHingeInset = ["Card hinge, inset", "вкладная"],
    ClassCardHingeHalf = ["Card hinge, half-overlay", "полунакладная"],
    ClassCupHinge = ["Cup hinge", "Петля чашечная"],
    ClassCupHingeOverlay = ["Cup hinge, overlay", "накладная"],
    ClassCupHingeInset = ["Cup hinge, inset", "вкладная"],
    ClassCupHingeHalf = ["Cup hinge, half-overlay", "полунакладная"],
    ClassRunner = ["Runner", "Направляющая"],
    ClassRunnerRoller = ["Roller runner", "роликовая"],
    ClassRunnerBall = ["Ball-bearing runner", "шариковая"],
    ClassRunnerUndermount = ["Undermount runner", "скрытая"],
    ClassHandle = ["Handle", "Ручка"],
    ClassCamConnector = ["Cam connector", "Эксцентрик"],
    ClassShelfPin = ["Shelf pin", "Полкодержатель"],
    ClassLiftMechanism = ["Lift mechanism", "Подъёмный механизм"],
    ClassLock = ["Lock", "Замок"],
    ClassLightProfile = ["Light profile", "Профиль подсветки"],
    ClassAccessory = ["Accessory", "Аксессуар"],
    ClassSlidingSystem = ["Sliding door system", "Система раздвижных дверей"],
    ClassSink = ["Sink", "Мойка"],
    ClassHob = ["Hob", "Варочная панель"],
    ClassOven = ["Oven", "Духовой шкаф"],
    ClassExtractor = ["Extractor", "Вытяжка"],
    ClassDishwasher = ["Dishwasher", "Посудомоечная машина"],
    ClassRefrigerator = ["Refrigerator", "Холодильник"],
    ClassWasher = ["Washer", "Стиральная машина"],
    ClassWorktop = ["Worktop", "Столешница"],
    ClassPlinth = ["Plinth", "Цоколь"],
    ClassCornice = ["Cornice", "Карниз"],
    ClassWallPanel = ["Wall panel", "Стеновая панель"],
    ClassEdgeProfile = ["Edge profile", "Кромочный профиль"],
    ClassHangingRail = ["Hanging rail", "Штанга"],
    ClassFiller = ["Filler strip", "Добор"],
    ClassGeneralProduct = ["General product", "Изделие общего назначения"],

    // ── Properties card ─────────────────────────────────────────────────────
    PropertiesTitle = ["Properties", "Свойства"],
    NoProperties = ["No properties.", "Нет свойств."],
    KeyHint = ["key", "ключ"],
    Add = ["Add", "Добавить"],
    Unclassified = ["unclassified", "без классификации"],
    PropertyKindText = ["Text", "Текст"],
    PropertyKindInteger = ["Integer", "Целое"],
    PropertyKindBool = ["Bool", "Логическое"],
    PropertyKindLength = ["Length", "Длина"],
    PropertyKindAngle = ["Angle", "Угол"],
    PropertyKindClass = ["Class", "Класс"],
    ClassPickerSearch = ["Search by name, code or alias", "Поиск по названию, коду или синониму"],
    ClassPickerNoMatch = ["No class matches.", "Нет подходящего класса."],
    ClassPickerNotInRegistry = ["not in the registry", "нет в реестре"],
    ClassPickerHint = ["Assign a class from the registry", "Назначить класс из реестра"],

    // ── Small shared strings ────────────────────────────────────────────────
    MeshVertexNoun = ["vertex", "вершина"],
    MeshFaceNoun = ["face", "грань"],
    UiScale = ["UI scale", "Масштаб интерфейса"],
    UiScaleHint = [
        "Icons are rasterised at the new pixel size instead of stretched",
        "Значки растрируются в новом размере пикселя, а не растягиваются"
    ],
    ExtrudeRefused = ["The selected path cannot be extruded: {}", "Выбранный путь нельзя выдавить: {}"],
    LockedNode = [
        "{} is locked. Unlock it in the scene tree to change it.",
        "{} заблокирован. Снимите блокировку в дереве сцены, чтобы изменить его."
    ],
    TheNode = ["The node", "Узел"],
    MenuLocked = [
        "The node is locked; unlock it before changing it",
        "Узел заблокирован: снимите блокировку, чтобы изменить его"
    ],

    // ── Scene tree ──────────────────────────────────────────────────────────
    TreeBadgeComponent = ["component", "компонент"],
    TreeBadgeCamera = ["camera", "камера"],
    TreeBadgeLight = ["light", "свет"],
    TreeBadgeMesh = ["mesh", "сетка"],
    TreeBadgeContour = ["contour", "контур"],
    TreeBadgePath = ["path", "путь"],
    TreeBadgeContours = ["contours", "контуры"],
    TreeBadgeSphere = ["sphere", "сфера"],
    TreeBadgeCone = ["cone", "конус"],
    TreeAddChild = ["Add child", "Добавить дочерний"],
    TreeAddChildHint = ["Create the next node inside this container", "Следующий созданный узел попадёт в этот контейнер"],
    TreeShow = ["Show", "Показать"],
    TreeHide = ["Hide", "Скрыть"],
    TreeUnlock = ["Unlock", "Снять блокировку"],
    TreeLock = ["Lock", "Заблокировать"],
    TreeIsolate = ["Isolate", "Изолировать"],
    TreeIsolateHint = ["Hide every other branch of the scene", "Скрыть все остальные ветви сцены"],
    TreeShowAll = ["Show all", "Показать всё"],
    TreeShowAllHint = ["Show every hidden node again", "Снова показать все скрытые узлы"],
    TreeOpenProduct = ["Open the product", "Открыть изделие"],
    TreeCloseProduct = ["Close the product", "Закрыть изделие"],
    TreeActiveProduct = ["Open for parts", "Открыто для деталей"],

    // ── Product card ────────────────────────────────────────────────────────
    ProductCardTitle = ["New product frame", "Новый каркас изделия"],
    Height = ["Height", "Высота"],
    MountFloor = ["On the floor", "На полу"],
    MountWall = ["On the wall", "На стене"],

    // ── Container card ──────────────────────────────────────────────────────
    ContainerRoleComponent = ["Component", "Компонент"],
    ContainerRoleGroup = ["Group", "Группа"],
    ContainerComponentDescription = [
        "A furniture or industrial component. It keeps its own identity and must be converted to a group before Ungroup will flatten it.",
        "Мебельный или промышленный компонент. Он сохраняет свою идентичность, и перед разгруппировкой его нужно преобразовать в группу."
    ],
    ContainerGroupDescription = [
        "An organisational container with no furniture type. Ungroup flattens it into its parent without moving any child in world space.",
        "Организационный контейнер без мебельного типа. Разгруппировка переносит его содержимое в родителя, не сдвигая дочерние узлы в мировом пространстве."
    ],
    SelectNodeInTree = ["Select a node in the scene tree.", "Выберите узел в дереве сцены."],
    SectionName = ["Name", "Имя"],
    NameMustNotBeEmpty = ["{} name must not be empty.", "Имя «{}» не должно быть пустым."],
    SectionContainer = ["Container", "Контейнер"],
    RootDescription = [
        "Root; it defines the world frame and is never a furniture component.",
        "Корень; задаёт мировую систему координат и не является мебельным компонентом."
    ],
    ChildrenCount = ["Children  {}", "Дочерние узлы: {}"],
    SectionCreateInside = ["Create inside", "Создать внутри"],
    AddChildHint = ["The next created node lands in this container", "Следующий созданный узел попадёт в этот контейнер"],

    // ── Placement and pivot cards ───────────────────────────────────────────
    Origin = ["Origin", "Начало координат"],
    OriginLocalTo = ["Origin (local to {})", "Начало (относительно {})"],
    OriginLocalToParent = ["Origin (local to the parent)", "Начало (относительно родителя)"],
    OriginOutOfRange = [
        "The stored origin is outside the editable millimetre range.",
        "Сохранённое начало выходит за редактируемый диапазон миллиметров."
    ],
    SectionTransform = ["Transform", "Преобразование"],
    WorldOrigin = ["World origin  {}", "Мировое начало: {}"],
    WorldOriginOutOfRange = ["World origin  outside the editable range", "Мировое начало: вне редактируемого диапазона"],
    RotationWithOrder = ["Rotation ({})", "Поворот ({})"],
    Apply = ["Apply", "Применить"],
    NodeNotInScene = ["node {} is not in the scene", "узел {} отсутствует в сцене"],
    FieldWithError = ["{}: {}", "{}: {}"],
    RotateAxis = ["Rotate {}", "Поворот {}"],
    LengthErrorEmpty = ["enter a length in millimetres", "введите длину в миллиметрах"],
    LengthErrorInvalid = ["not a valid millimetre value", "недопустимое значение в миллиметрах"],
    LengthErrorTooPrecise = [
        "use at most three decimal places in millimetres",
        "не больше трёх знаков после запятой в миллиметрах"
    ],
    LengthErrorOutOfRange = ["length is outside the supported range", "длина выходит за поддерживаемый диапазон"],
    DegreesErrorEmpty = ["enter an angle in degrees", "введите угол в градусах"],
    DegreesErrorInvalid = ["not a valid degree value", "недопустимое значение в градусах"],
    DegreesErrorTooPrecise = [
        "use at most three decimal places in degrees",
        "не больше трёх знаков после запятой в градусах"
    ],
    DegreesErrorOutOfRange = ["angle is outside the supported range", "угол выходит за поддерживаемый диапазон"],
    NumberInvalid = ["enter a number", "введите число"],
    Pivot = ["Pivot", "Пивот"],
    PivotLocalOffset = ["Pivot (local offset)", "Пивот (локальное смещение)"],
    PanelPivotFollowsOrigin = ["Pivot follows origin (offset 0)", "Пивот следует за началом (смещение 0)"],
    PivotWorld = ["Pivot (world)  {}", "Пивот (мир): {}"],
    PivotWorldOutOfRange = ["Pivot (world)  outside the editable range", "Пивот (мир): вне редактируемого диапазона"],
    PivotInNodeFrame = ["Offset from the node's origin", "Смещение от начала узла"],
    PivotNotEditable = ["The stored pivot is not editable.", "Сохранённый пивот нельзя редактировать."],
    Center = ["Center", "Центр"],

    // ── Exact-distance fields ───────────────────────────────────────────────
    DistanceCentred = ["Centred", "По центру"],
    DistanceNoNeighbour = ["No surface in this direction", "В этом направлении нет поверхности"],
    DistanceOutOfVoid = ["The distance does not fit the void", "Расстояние не помещается в пустоту"],
    DistanceLockedFace = [
        "The opposite face is locked; this distance would resize the part",
        "Противоположная грань закреплена: это расстояние изменит размер детали"
    ],
    DistanceLocked = ["Locked at {} mm", "Закреплено: {} мм"],

    // ── The tool card ───────────────────────────────────────────────────────
    ToolGapLow = ["Low gap", "Нижний зазор"],
    ToolGapHigh = ["High gap", "Верхний зазор"],

    // ── The render place ────────────────────────────────────────────────────
    Render = ["Render", "Рендер"],
    RenderTitle = ["Final render", "Финальный рендер"],
    RenderHint = ["The final render of this scene", "Финальный рендер этой сцены"],
    RenderOf = ["A still image of “{}”, drawn by the final render.", "Неподвижное изображение «{}», нарисованное финальным рендером."],
    RenderPreset = ["{} — {}", "{} — {}"],
    RenderPresetDraft = ["Draft", "Черновик"],
    RenderPresetPrint = ["Print A4", "Печать A4"],
    RenderWidth = ["Width", "Ширина"],
    RenderHeight = ["Height", "Высота"],
    RenderSamples = ["Samples", "Сэмплы"],
    RenderExposure = ["Exposure", "Экспозиция"],
    RenderDestination = ["File", "Файл"],
    RenderNoPath = ["no file chosen yet", "файл ещё не выбран"],
    RenderChoose = ["Choose…", "Выбрать…"],
    RenderStart = ["Render", "Рендерить"],
    RenderCancel = ["Cancel", "Отменить"],
    RenderNeedsPath = ["Choose a file to write the image to.", "Выберите файл, куда записать изображение."],
    RenderNeedsSize = ["Name a size for the image.", "Укажите размер изображения."],
    RenderSamplesDone = ["{} of {} samples", "{} из {} сэмплов"],
    RenderDone = ["Written to {} — {}, {} samples", "Записано в {} — {}, {} сэмплов"],
    RenderCancelled = ["Cancelled; no image was written.", "Отменено; изображение не записано."],
    RenderFailed = ["The render stopped: {}", "Рендер остановлен: {}"],

    // ── Home ────────────────────────────────────────────────────────────────
    HomeClose = ["Close  (Esc)", "Закрыть  (Esc)"],
    HomeStartScene = ["Start a scene", "Начните сцену"],
    HomeNewScene = ["New scene", "Новая сцена"],
    HomeOpenScene = ["Open scene…", "Открыть сцену…"],
    HomeRecentFiles = ["Recent files", "Недавние файлы"],
    HomeNoScene = ["No scene opened yet.", "Пока нет открытых сцен."],
    HomeEmptyTemplate = ["Empty template", "Пустой шаблон"],
    HomeEmptyTemplateHint = [
        "A fresh scene with the default catalogue and nothing else.",
        "Новая сцена со стандартным каталогом и больше ничем."
    ],
    HomeUseTemplate = ["Use template", "Взять шаблон"],
    HomeUnnamed = ["(unnamed)", "(без имени)"],

    // ── Command palette ─────────────────────────────────────────────────────
    PaletteHeading = ["Commands", "Команды"],
    PaletteSearch = ["Search commands…", "Поиск команд…"],
    PaletteNoMatch = ["No command matches that search.", "Ни одна команда не подходит под запрос."],
    PaletteCountOne = ["{} command", "{} команда"],
    PaletteCountFew = ["{} commands", "{} команды"],
    PaletteCountMany = ["{} commands", "{} команд"],
    PaletteUnknownCommand = ["Unknown command `{}`.", "Неизвестная команда `{}`."],
    PaletteNotAllowed = ["The current selection does not allow this.", "Текущее выделение не позволяет это."],
    PaletteNeedsSelection = ["Needs a selection.", "Нужно выделение."],
    PaletteNeedsNewName = [
        "Needs a new name; rename the node in the inspector.",
        "Нужно новое имя; переименуйте узел в инспекторе."
    ],
    PaletteNeedsTwoNodes = ["Needs two or more selected nodes.", "Нужно два или больше выделенных узла."],
    PaletteNeedsCamera = ["Needs a selected camera.", "Нужна выделенная камера."],
    PaletteAddPanelMaterial = [
        "Add a panel material to the catalogue first.",
        "Сначала добавьте материал панели в каталог."
    ],
    GeneratedGroup = ["Group", "Группа"],

    // ── Drawing tools ───────────────────────────────────────────────────────
    ToolLine = ["Line", "Линия"],
    ToolPolyline = ["Polyline", "Полилиния"],
    ToolArc = ["Arc", "Дуга"],
    ToolArcFromCentre = ["Arc from centre", "Дуга от центра"],
    ToolRectangle = ["Rectangle", "Прямоугольник"],
    ToolRectangleFromCentre = ["Rectangle from centre", "Прямоуг.центр"],
    ToolRectangleFromCentreDirected = ["Directed rectangle", "Прямоуг.ось"],
    ToolCircle = ["Circle", "Окружность"],
    ToolCircleThreePoints = ["Circle through 3 points", "Окружн.3 точки"],
    ToolRegularPolygon = ["Polygon", "Многоугольник"],
    DrawingName = ["{} drawing", "{} — эскиз"],
    PolylineStripLabel = [
        "Polyline · RMB line/arc · Alt ignores snaps",
        "Полилиния · ПКМ линия/дуга · Alt игнорирует привязки"
    ],
    DraftSegmentLine = ["Straight segment", "Прямой сегмент"],
    DraftSegmentArc = ["Arc segment", "Дуговой сегмент"],
    DraftSegmentCurrent = ["The next segment is already this kind", "Следующий сегмент уже такой"],
    DraftFinish = ["Finish the drawing", "Завершить эскиз"],
    DraftNeedsTwo = ["A drawing needs two segments to finish", "Чтобы завершить эскиз, нужно два сегмента"],

    // ── Ribbon ──────────────────────────────────────────────────────────────
    RibbonFurniture = ["Furniture", "Мебель"],
    RibbonHardware = ["Hardware", "Фурнитура"],
    RibbonModel = ["Model", "Модель"],
    RibbonSketch = ["Sketch", "Эскиз"],
    RibbonScene = ["Scene", "Сцена"],
    RibbonView = ["View", "Вид"],

    // The ribbon's groups, in the order the map draws them (U68).
    GroupCreate = ["Create", "Создать"],
    GroupModify = ["Modify", "Изменить"],
    GroupConstruction = ["Construction", "Опорные"],
    GroupDimensions = ["Dimensions", "Размеры"],
    GroupMeasure = ["Measure", "Измерение"],
    GroupBooleans = ["Booleans", "Булевы"],
    GroupContourBooleans = ["Contours", "Контуры"],
    ContourBooleanBodiesOnModel = [
        "A body is cut by the Model tab's booleans",
        "Тело режется булевыми операциями вкладки «Модель»"
    ],
    GroupDraft = ["Draft", "Черновик"],
    GroupPrimitives = ["Primitives", "Примитивы"],
    GroupFaces = ["Faces", "Грани"],
    GroupBodies = ["Bodies", "Тела"],
    GroupProduct = ["Product", "Изделие"],
    GroupPanels = ["Panels", "Панели"],
    GroupMachining = ["Machining", "Обработка"],
    GroupFasteners = ["Fasteners", "Крепёж"],
    GroupHardware = ["Hardware", "Фурнитура"],
    GroupChecks = ["Checks", "Проверка"],
    FillNeedsProduct = ["Open a product first", "Сначала откройте изделие"],
    FillNeedsMaterial = ["Add a panel material first", "Сначала добавьте материал панели"],
    FillNeedsFillTrait = ["This class does not fill a void", "Этот класс не заполняет пустоту"],
    PanelContourDraw = ["{}: click the outline, then close the ring", "{}: наметьте контур, затем замкните его"],
    PanelContourOpen = ["A panel is cut from a closed outline", "Панель вырезается по замкнутому контуру"],
    PanelContourUnusable = ["This outline is not a usable panel shape", "Этот контур не годится для панели"],
    ToolMethodName = ["{}: {}", "{}: {}"],
    FillMethodFit = ["Largest rectangle", "Наибольший прямоугольник"],
    FillMethodShape = ["Niche shape", "По форме ниши"],
    GroupObjects = ["Objects", "Объекты"],
    GroupViews = ["Views", "Виды"],
    GroupMaterials = ["Materials", "Материалы"],
    GroupProjection = ["Projection", "Проекция"],
    GroupDisplay = ["Display", "Отображение"],
    GroupLayout = ["Layout", "Раскладка"],
    GroupNavigation = ["Navigation", "Навигация"],
    GroupSnaps = ["Snaps", "Привязки"],

    // The ribbon's tools (U68); a planned row is named here before its command exists.
    ToolProductFrame = ["Product frame", "Каркас изделия"],
    ToolProductFrameDraw = ["Frame by drawing", "Каркас по чертежу"],
    ToolProductFrameContour = ["Frame from contour", "Каркас по контуру"],
    ToolPanel = ["Panel", "Панель"],
    ToolPost = ["Post", "Стойка"],
    ToolVoid = ["Niche", "Ниша"],
    ToolShelf = ["Shelf", "Полка"],
    ToolWall = ["Wall", "Стенка"],
    ToolContourPanel = ["Panel from contour", "Панель по контуру"],
    ToolAssembly = ["Assembly", "Сборка"],
    ToolGroupNode = ["Group", "Группа"],
    ToolCamera = ["Camera", "Камера"],
    ToolLight = ["Light", "Свет"],
    ToolNamedView = ["Named view", "Именованный вид"],
    ToolStandardView = ["Standard view", "Стандартный вид"],
    ToolMaterialCatalogue = ["Material catalogue", "Каталог материалов"],
    ToolAssignMaterial = ["Assign material", "Назначить материал"],
    ToolFrameAll = ["Frame all", "Показать всё"],
    ToolProjection = ["Projection", "Проекция"],
    ToolStyle = ["Display style", "Стиль отображения"],
    ToolContours = ["Contours", "Контуры"],
    ToolShadows = ["Shadows", "Тени"],
    ToolSection = ["Section", "Сечение"],
    ToolLayoutOne = ["One view", "Один вид"],
    ToolLayoutTwo = ["Two views", "Два вида"],
    ToolLayoutFour = ["Four views", "Четыре вида"],
    ToolMaximize = ["Maximize the view", "Развернуть вид"],
    ToolOrbit = ["Orbit", "Орбита"],
    ToolPan = ["Pan", "Панорама"],
    ToolZoom = ["Zoom", "Масштаб"],
    SwitchProjection = ["Switch the projection", "Переключить проекцию"],
    ToolOrthographic = ["Orthographic", "Ортогональная"],
    ToolPerspective = ["Perspective", "Перспективная"],
    ToolExtrude = ["Extrude", "Выдавить"],
    ToolRevolve = ["Revolve", "Вращение"],
    ToolLoft = ["Loft", "По сечениям"],
    ToolSweep = ["Sweep", "По траектории"],
    ToolBox = ["Box", "Блок"],
    ToolCylinder = ["Cylinder", "Цилиндр"],
    ToolSphere = ["Sphere", "Сфера"],
    ToolCentreRadius = ["Centre and radius", "Центр и радиус"],
    ToolCentreDiameter = ["Centre and diameter", "Центр и диаметр"],
    ToolThreePoints = ["Through 3 points", "Через 3 точки"],
    ToolCone = ["Cone", "Конус"],
    ToolPyramid = ["Pyramid", "Пирамида"],
    ToolUnion = ["Union", "Объединить"],
    ToolSubtract = ["Subtract", "Вычесть"],
    ToolIntersect = ["Intersect", "Пересечение"],
    BooleanNeedsBody = [
        "A body boolean needs two constructive bodies: pick the target, then its tool.",
        "Булева операция над телами требует два конструктивных тела: укажите целевое, затем инструмент."
    ],
    BooleanNeedsRing = [
        "Select a closed contour first: the boolean is born in its frame",
        "Сначала выделите замкнутый контур: в его системе координат родится результат"
    ],
    BooleanPick = ["Contours picked: {0}", "Выбрано контуров: {0}"],
    BooleanPickBody = ["Bodies picked: {0}", "Выбрано тел: {0}"],
    ToolFillet = ["Fillet", "Скругление"],
    ToolChamfer = ["Chamfer", "Фаска"],
    ToolShell = ["Shell", "Оболочка"],
    ToolOffsetFace = ["Offset face", "Сместить грань"],
    ToolDeleteFace = ["Delete face", "Удалить грань"],
    ToolJoin = ["Join", "Соединить"],
    ToolSplit = ["Split", "Разделить"],
    ToolScale = ["Scale", "Масштаб"],
    ToolMirror = ["Mirror", "Зеркало"],
    ToolTrim = ["Trim", "Обрезать"],
    ToolExtend = ["Extend", "Удлинить"],
    ToolOffset = ["Offset", "Сместить"],
    ToolBreak = ["Break", "Разорвать"],
    ToolConstructionHorizontal = ["Horizontal reference", "Опорная горизонталь"],
    ToolConstructionVertical = ["Vertical reference", "Опорная вертикаль"],
    ToolConstructionParallel = ["Parallel reference", "Опорная параллель"],
    ToolConstructionAngle = ["Angled reference", "Опорная под углом"],
    ToolDimensionLinear = ["Linear dimension", "Линейный размер"],
    ToolDimensionAngular = ["Angular dimension", "Угловой размер"],
    ToolDimensionRadius = ["Radial dimension", "Радиальный размер"],
    ToolMeasure = ["Measure", "Измерение"],
    ToolMeasureLength = ["Length", "Длина"],
    ToolDrill = ["Drill", "Сверление"],
    ToolGroove = ["Groove", "Паз"],
    ToolEdgeband = ["Edge banding", "Кромка"],
    ToolHingeHole = ["Hinge hole", "Отверстие петли"],
    ToolConfirmat = ["Confirmat", "Конфирмат"],
    ToolCam = ["Cam lock", "Эксцентрик"],
    ToolDowel = ["Dowel", "Шкант"],
    ToolMinifix = ["Minifix", "Минификс"],
    ToolShelfPin = ["Shelf pin", "Полкодержатель"],
    ToolHinge = ["Hinge", "Петля"],
    ToolRunner = ["Runner", "Направляющая"],
    ToolHandle = ["Handle", "Ручка"],
    ToolFoot = ["Foot", "Опора"],
    ToolCollisions = ["Collisions", "Пересечения"],
    ToolPartsList = ["Parts list", "Список деталей"],
    ToolCutList = ["Cut list", "Раскрой"],

    // The ribbon's variants: the input methods of one tool (U67).
    ToolCircleTangent = ["Tangent to two lines", "Касательно двух линий"],
    ToolArcThreePoints = ["Through 3 points", "По 3 точкам"],
    ToolLineAngle = ["By angle", "Под углом"],
    ToolPolygonEdge = ["By edge", "По ребру"],
    ToolOffsetDistance = ["By distance", "На расстояние"],
    ToolOffsetThrough = ["Through a point", "Через точку"],
    ToolFilletRadius = ["By radius", "По радиусу"],
    ToolFilletTwoLines = ["Two lines", "Две линии"],
    ToolChamferDistance = ["By distance", "На расстояние"],
    ToolChamferTwoLines = ["Two lines", "Две линии"],
    ToolDiameter = ["Diameter", "Диаметр"],
    ToolCircleCentreRadius = ["Centre and radius", "Центр и радиус"],
    ToolCircleCentreDiameter = ["Centre and diameter", "Центр и диаметр"],
    ToolArcStartEndRadius = ["Start, end and radius", "Начало, конец и радиус"],
    ToolRectangleFromCorner = ["From corner", "От угла"],
    ToolLineTwoPoints = ["Two points", "По двум точкам"],
    ToolPolygonRadius = ["By radius", "По радиусу"],
    ToolPolygonApothem = ["By apothem", "По апофеме"],
    ToolRectangleFromEdge = ["From edge", "От ребра"],
    PinMethod = ["Use this as the main method", "Сделать основным способом"],
    MethodPinned = ["The main method", "Основной способ"],
    ToolMethods = ["All methods", "Все способы"],

    FieldSides = ["Sides", "Стороны"],
    FieldSize = ["Size", "Размер"],
    FieldHeight = ["Height", "Высота"],
    Close = ["Close", "Закрыть"],
    Cancel = ["Cancel", "Отмена"],
    DraftCloseHint = ["Close the draft into a ring", "Замкнуть черновик в кольцо"],
    DraftCloseNeedsTwo = ["A draft needs two segments to close", "Чтобы замкнуть черновик, нужно два сегмента"],
    DraftCancelHint = ["Cancel the draft", "Отменить черновик"],
    DraftNotRunning = ["No draft is running", "Черновик не запущен"],
    ExtrudeSelectPath = ["Select a closed path first", "Сначала выберите замкнутый путь"],
    ExtrudeSelectedPath = ["Extrude the selected path", "Выдавить выбранный путь"],
    ExtrudePathOpen = ["The selected path is open; close it first", "Выбранный путь разомкнут; сначала замкните его"],
    ExtrudeNotPath = ["The selected object is not a path", "Выбранный объект не является путём"],
    ExtrudeSelectedDrawing = ["Pull an area of the selected drawing", "Вытянуть область выбранного контура"],
    AreaOutside = ["Point at an area of the drawing", "Укажите область на контуре"],
    AreaHole = [
        "The area under the pointer has a hole, and a body carries one ring yet",
        "Область под курсором имеет отверстие, а тело пока несёт один контур"
    ],
    AreaBodyName = ["{} area", "{} — область"],
    PrimitiveDrawBase = ["{}: draw its base on the work plane", "{}: начертите основание на рабочей плоскости"],
    PrimitiveDrawRound = ["{}: draw the round it is built around", "{}: начертите окружность, вокруг которой он строится"],
    BodyProfileOpen = ["A body is swept from a closed profile", "Тело выдавливается из замкнутого профиля"],
    FrameHeight = ["Frame height", "Высота каркаса"],

    // ── The directory application ──────────────────────────────────────────
    DirectoryTitle = ["Nomenclature directory", "Каталог номенклатуры"],
    DirectoryFolder = ["Folder", "Папка"],
    DirectoryReload = ["Reload", "Перечитать"],
    DirectoryNewItem = ["New item", "Новый предмет"],
    DirectorySave = ["Save", "Сохранить"],
    DirectoryItems = ["Items", "Предметы"],
    DirectoryRegistry = ["Registry", "Реестр"],
    DirectoryFilter = ["Filter", "Фильтр"],
    DirectoryName = ["Name", "Название"],
    DirectorySlug = ["Slug", "Код"],
    DirectoryKind = ["Kind", "Вид"],
    DirectoryIdentifier = ["Id", "Идентификатор"],
    DirectoryArchived = ["Archived", "Снят с производства"],
    DirectoryPackage = ["Package", "Пакет"],
    DirectoryProperties = ["Properties", "Свойства"],
    DirectoryPickItem = ["Pick an item, or create one", "Выберите предмет или создайте новый"],
    DirectorySaved = ["Saved, and the revision moved", "Сохранено, ревизия сменилась"],
    DirectoryNothingSaved = ["Nothing is saved yet", "Пока ничего не сохранено"],
    DirectoryRequired = ["required", "обязательное"],
    DirectoryReadOnly = ["read-only", "только чтение"],
    DirectoryEmpty = ["The directory carries no package", "В каталоге нет ни одного пакета"],
    DirectoryImportPrices = ["Import prices", "Импорт цен"],
    PriceNoCurrency = ["no currency in this package", "в пакете нет валюты"],
    PriceFresh = ["{} days old", "возраст {} дней"],
    PriceStale = ["{} days old, past the {} day limit", "{} дней, больше предела в {} дней"],
    PriceUndated = ["no date", "даты нет"],
    PriceListTitle = ["Import a price list", "Импорт прайс-листа"],
    PriceListFiles = ["Price list (CSV)", "Прайс-лист (CSV)"],
    PriceListUnread = ["Cannot read the price list: {}", "Не удалось прочитать прайс-лист: {}"],
    PriceListRefused = ["No price was written: {}", "Ни одна цена не записана: {}"],

    // ── Display names ───────────────────────────────────────────────────────
    Axonometric = ["Axonometric", "Аксонометрия"],
    Front = ["Front", "Спереди"],
    Left = ["Left", "Слева"],
    Top = ["Top", "Сверху"],
    Right = ["Right", "Справа"],
    Bottom = ["Bottom", "Снизу"],
    ShadingEdgesCaption = ["Edges", "Рёбра"],
    ShadingShadedCaption = ["Shaded", "Заливка"],
    ShadingWireCaption = ["Wire", "Каркас"],
    ShadingHiddenCaption = ["Hidden", "Скрытые"],
    ShadingGhostCaption = ["Ghost", "Призрак"],
    ShadingShadedWithEdges = ["Shaded with edges", "Заливка с рёбрами"],
    ShadingShaded = ["Shaded", "Заливка"],
    ShadingWireframe = ["Wireframe", "Каркас"],
    ShadingHiddenEdges = ["Shaded with hidden edges", "Заливка со скрытыми рёбрами"],
    ShadingTranslucent = ["Translucent", "Полупрозрачный"],

    // ── Workspace settings ──────────────────────────────────────────────────
    SettingsTitle = ["Settings", "Настройки"],
    Done = ["Done", "Готово"],
    SectionAppearance = ["Appearance", "Оформление"],
    SectionViewport = ["Viewport", "Область просмотра"],
    SectionPanels = ["Panels", "Панели"],
    SectionShortcuts = ["Shortcuts", "Горячие клавиши"],
    SectionAssistant = ["Assistant", "Ассистент"],
    AttachChannel = ["Let a local agent reach this window", "Разрешить локальному агенту доступ к окну"],
    AttachChannelHint = [
        "An agent on this computer reaches the window through a token file in the settings folder, and the switch closes that door. Nothing is reachable from a network, and the token dies with the window.",
        "Агент на этом компьютере получает доступ к окну через файл с токеном в папке настроек, и выключатель закрывает эту дверь. Из сети окно недоступно, а токен живёт только пока открыто окно."
    ],
    Theme = ["Theme", "Тема"],
    ThemeDark = ["Dark", "Тёмная"],
    ThemeLight = ["Light", "Светлая"],
    ThemeSystem = ["System", "Системная"],
    Palette = ["Palette", "Палитра"],
    PaletteGraphiteCopper = ["Graphite · copper", "Графит · медь"],
    PaletteInkMagenta = ["Ink · magenta", "Чернила · маджента"],
    PaletteSlateViolet = ["Slate · violet", "Сланец · фиалка"],
    PaletteHint = [
        "A preset paints the whole window in both variants; the accent is the one colour you can replace.",
        "Палитра окрашивает всё окно в обоих вариантах; акцент — единственный цвет, который можно заменить."
    ],
    Accent = ["Accent", "Акцент"],
    AccentHint = [
        "One accent for the dark and the light window; Reset returns to the palette's own.",
        "Один акцент для тёмного и светлого окна; сброс возвращает акцент палитры."
    ],
    Language = ["Language", "Язык"],
    LanguageEnglish = ["English", "English"],
    LanguageRussian = ["Русский", "Русский"],
    ScaleHint = [
        "The scale applies to the whole window; icons are rasterised at the new pixel size.",
        "Масштаб применяется ко всему окну; значки растрируются в новом размере."
    ],
    DisplayStyle = ["Display style", "Стиль отображения"],
    OpeningView = ["Opening view", "Начальный вид"],
    RollRingStep = ["Roll ring step", "Шаг кольца крена"],
    RollRingStepHint = [
        "Degrees per step when the ring under the corner widget is turned by hand.",
        "Градусов за шаг при ручном повороте кольца под виджетом ориентации."
    ],
    GizmoHandleLength = ["Handle length", "Длина ручек"],
    GizmoHandleLengthHint = [
        "The screen length of the translate handles, in points; a small part stays readable.",
        "Длина ручек перемещения на экране в пунктах; маленькая деталь остаётся видна."
    ],
    GizmoRingRadius = ["Ring radius", "Радиус колец"],
    GizmoRingRadiusHint = [
        "The rotate ring radius as a percentage of the camera distance.",
        "Радиус колец поворота в процентах от расстояния до камеры."
    ],
    WorkCursor = ["Work cursor", "Рабочий курсор"],
    CursorSize = ["Cursor size", "Размер курсора"],
    CursorSizeHint = [
        "The side of the square the shell draws at the pointer, in points.",
        "Сторона квадрата, который оболочка рисует у указателя, в пунктах."
    ],
    CursorCrosshair = ["Draw a full crosshair", "Рисовать полное перекрестие"],
    CursorCrosshairHint = [
        "One hairline across the view in both directions, for lining up two boards.",
        "Одна тонкая линия через весь вид в обе стороны, чтобы выравнивать две доски."
    ],
    CursorHint = [
        "The scene hides the system arrow and draws this square; a gesture that has a cursor of its own keeps it.",
        "Сцена скрывает системную стрелку и рисует этот квадрат; жест со своим курсором сохраняет его."
    ],
    ShowFloorGrid = ["Show the floor grid", "Показывать сетку пола"],
    Snapping = ["Snapping", "Привязка"],
    SnapPlanes = ["Coordinate planes", "Координатные плоскости"],
    SnapPlanesHint = [
        "X = 0, Y = 0 and Z = 0 capture a dragged piece, the floor included; until a wall tool exists they stand in for the walls of a room.",
        "X = 0, Y = 0 и Z = 0 захватывают перетаскиваемую деталь, включая пол; пока нет инструмента стен, они заменяют стены комнаты."
    ],
    SnapFrames = ["Neighbouring frames", "Соседние корпуса"],
    SnapFramesHint = [
        "The virtual faces of the frames around the piece capture it, so two cabinets meet flush.",
        "Виртуальные грани окружающих корпусов захватывают деталь, поэтому два шкафа встают вплотную."
    ],
    SnapAll = ["All snaps", "Все привязки"],
    SnapAllHint = [
        "Turns every snap on, or every snap off: a pointer that fights the geometry gets the hand back in one click.",
        "Включает все привязки или выключает их разом: курсор, который спорит с геометрией, отдаёт руку одним нажатием."
    ],
    SnapMode = ["Snap mode", "Режим привязки"],
    SnapModeHint = [
        "How a snap reads a corner above the drawing plane: 2.5D takes its foot on the plane, 3D takes the point itself. The cell shows the reading in force; press to switch.",
        "Как привязка читает угол над плоскостью чертежа: 2.5D берёт его основание на плоскости, 3D — саму точку. Клетка показывает действующий режим; нажатие переключает."
    ],
    SnapModeSpace = ["3D — the point itself", "3D — сама точка"],
    SnapModeSpaceHint = [
        "A tool that draws on a plane captures only a point that lies in it, so a figure is never built from a place it cannot contain.",
        "Инструмент, который чертит на плоскости, захватывает только точку, лежащую в ней, поэтому фигура никогда не строится из места, которого не может содержать."
    ],
    SnapModeFoot = ["2.5D — the foot on the plane", "2.5D — основание на плоскости"],
    SnapModeFootHint = [
        "A corner anywhere captures, and the point is its foot on the drawing plane: a top view lines a board up with the shadow it casts.",
        "Захватывает угол из любого места, а точка — его основание на плоскости чертежа: вид сверху совмещает доску с её тенью."
    ],
    SnapVertex = ["Corner", "Угол"],
    SnapVertexHint = [
        "A corner of a panel, a body or a frame, and a point the drawing already has.",
        "Угол панели, тела или корпуса и уже поставленная точка чертежа."
    ],
    SnapIntersection = ["Crossing", "Пересечение"],
    SnapIntersectionHint = [
        "Where two edges cross as the drawing plane sees them.",
        "Место, где два ребра пересекаются в плоскости чертежа."
    ],
    SnapMidpoint = ["Middle", "Середина"],
    SnapMidpointHint = ["The middle of an edge.", "Середина ребра."],
    SnapCentre = ["Centre", "Центр"],
    SnapCentreHint = [
        "The centre of a face, a box, a primitive or a circle.",
        "Центр грани, коробки, тела или окружности."
    ],
    SnapPerpendicular = ["Perpendicular", "Перпендикуляр"],
    SnapPerpendicularHint = [
        "The foot of the perpendicular from the previous point onto an edge.",
        "Основание перпендикуляра из предыдущей точки на ребро."
    ],
    SnapTangent = ["Tangent", "Касание"],
    SnapTangentHint = [
        "Where a line from the previous point touches a circle in the drawing plane.",
        "Точка, где линия из предыдущей точки касается окружности в плоскости чертежа."
    ],
    SnapEdge = ["On an edge", "На ребре"],
    SnapEdgeHint = ["A point anywhere along an edge.", "Точка в любом месте ребра."],
    SnapFace = ["On a face", "На грани"],
    SnapFaceHint = [
        "The surface the pointer points at, as the ruler measures it.",
        "Поверхность, на которую указывает курсор, как её измеряет линейка."
    ],
    SnapAxis = ["World axes", "Мировые оси"],
    SnapAxisHint = [
        "A point on the world X, Y or Z axis, where the drawing plane holds it.",
        "Точка на мировой оси X, Y или Z там, где её держит плоскость чертежа."
    ],
    SnapGrid = ["Grid", "Сетка"],
    SnapGridHint = [
        "The drawing plane's 10 mm grid.",
        "Сетка плоскости чертежа с шагом 10 мм."
    ],
    SnapCaptured = ["Snap: {}", "Привязка: {}"],
    SnapTolerance = ["Tolerance", "Допуск"],
    SnapToleranceHint = [
        "How near a surface has to be to capture the drag, in millimetres; the piece lands flush on it, because the tolerance is where the snap fires.",
        "Насколько близко должна быть поверхность, чтобы захватить перетаскивание, в миллиметрах; деталь встаёт вплотную, потому что допуск — это расстояние срабатывания привязки."
    ],
    FloorStop = ["Floor stops every body", "Пол останавливает любое тело"],
    FloorStopHint = [
        "A dragged body stops at the invisible floor at Z = 0 whatever the two switches above say; a class that keeps its height is unchanged either way.",
        "Перетаскиваемое тело останавливается на невидимом полу Z = 0, что бы ни говорили два переключателя выше; изделие, которое держит высоту по классу, не меняется."
    ],
    ShowEveryTriangleEdge = ["Show every triangle edge", "Показывать все рёбра треугольников"],
    AllEdgesHint = [
        "Diagnostic: keeps the triangulation diagonals the form-edge view hides.",
        "Диагностика: оставляет диагонали триангуляции, которые скрывает вид форменных рёбер."
    ],
    ViewportDefaultsHint = [
        "These are the defaults of a new viewport; the open view keeps its own state.",
        "Это настройки нового вида; открытый вид сохраняет своё состояние."
    ],
    PanelOpacity = ["Panel opacity", "Непрозрачность панелей"],
    PanelOpacityHint = [
        "Lower values let the scene show through the chrome drawn over it.",
        "Меньшие значения позволяют сцене просвечивать сквозь панели над ней."
    ],
    ScenePanelVisible = ["Scene panel", "Панель сцены"],
    InspectorPanelVisible = ["Inspector panel", "Панель инспектора"],
    ResetPanels = ["Reset panels", "Сбросить панели"],
    PanelsHint = [
        "Panels float over the scene; the header buttons collapse or hide both at once.",
        "Панели плавают над сценой; кнопки в шапке сворачивают или скрывают обе сразу."
    ],
    ShortcutHidePanels = ["Hide all panels", "Скрыть все панели"],
    ShortcutCollapsePanels = ["Collapse all panels", "Свернуть все панели"],
    IconColor = ["Icon colour", "Цвет значков"],
    IconColorAuto = ["Automatic", "Автоматически"],
    IconColorAlways = ["Always", "Всегда"],
    IconColorNever = ["Never", "Никогда"],
    IconColorHint = [
        "Icons take their hue from the semantic family; Automatic falls back to monochrome below a 20 px raster.",
        "Значки берут оттенок из смыслового семейства; «Автоматически» переходит в монохромный при растре меньше 20 px."
    ],
    ResetAppearance = ["Reset", "Сбросить"],
    ResetAppearanceHint = [
        "Restores the default palette, accent, opacity and icon colour.",
        "Возвращает палитру, акцент, прозрачность и цвет значков по умолчанию."
    ],
    ShortcutsHint = [
        "The same keys the ribbon, the header and the settings page name.",
        "Те же клавиши, что названы на ленте, в заголовке и в строке состояния."
    ],
    ShortcutNewScene = ["Open an empty scene in a new tab", "Открыть пустую сцену в новой вкладке"],
    ShortcutCloseTab = ["Close the active tab", "Закрыть активную вкладку"],
    ShortcutSave = ["Save the scene", "Сохранить сцену"],
    ShortcutUndo = ["Undo", "Отменить"],
    ShortcutRedo = ["Redo", "Повторить"],
    ShortcutPalette = ["Command palette", "Палитра команд"],
    ShortcutProjection = ["Perspective / orthographic", "Перспектива / ортогональ"],
    ShortcutGrid = ["Toggle the drawing grid", "Переключить сетку рисования"],
    ShortcutCancel = ["Cancel the current tool", "Отменить текущий инструмент"],
    ShortcutOrbit = ["Orbit", "Облёт"],
    ShortcutPan = ["Pan", "Панорама"],
    ShortcutZoom = ["Zoom", "Масштаб"],

    // ── Commands (engine) ────────────────────────────────────────
    CmdCreateCamera = ["Create camera", "Создать камеру"],
    CmdCreateLight = ["Create light", "Создать источник света"],
    CmdSetActiveCamera = ["Set active camera", "Сделать камеру активной"],
    CmdSetCameraProjection = ["Set camera projection", "Задать проекцию камеры"],
    CmdSetLightParameters = ["Set light parameters", "Задать параметры света"],
    CmdCreateAssembly = ["Create assembly", "Создать сборку"],
    CmdCreateProduct = ["Create product frame", "Создать каркас изделия"],
    CreationNeedsProduct = ["Create a product first.", "Сначала создайте изделие."],
    CreationProductClosed = [
        "The product is closed; open it before adding a part.",
        "Изделие закрыто; откройте его, прежде чем добавлять деталь."
    ],
    CmdSetProductEnvelope = ["Set product frame size", "Задать габарит изделия"],
    CmdRename = ["Rename", "Переименовать"],
    CmdDelete = ["Delete", "Удалить"],
    CmdDeleteSelection = ["Delete selection", "Удалить выделение"],
    CmdDuplicate = ["Duplicate", "Дублировать"],
    CmdGroup = ["Group", "Сгруппировать"],
    CmdUngroup = ["Ungroup", "Разгруппировать"],
    CmdPlace = ["Place", "Разместить"],
    CmdPlaceAtWorldZero = ["Place at world zero", "На начало мира"],
    CmdPlaceSelection = ["Place selection", "Разместить выделение"],
    CmdSetPivot = ["Set pivot", "Задать пивот"],
    CmdCenterPivot = ["Center pivot", "Центрировать пивот"],
    CmdRebaseOrigin = ["Rebase origin", "Переякорить начало"],
    CmdSetProperty = ["Set property", "Задать свойство"],
    CmdSetProperties = ["Set properties", "Задать свойства"],
    CmdClearProperty = ["Clear property", "Очистить свойство"],
    CmdConvertToGroup = ["Convert to group", "Преобразовать в группу"],
    CmdMoveVertices = ["Move vertices", "Переместить вершины"],
    CmdCreatePrimitive = ["Create primitive", "Создать примитив"],
    CmdReplacePrimitive = ["Replace primitive parameters", "Заменить параметры примитива"],
    CmdCreateExtrusion = ["Create extrusion", "Создать выдавливание"],
    CmdReplaceContour = ["Replace contour", "Заменить контур"],
    CmdExtrudePath = ["Extrude path", "Выдавить путь"],
    CmdExtrudeArea = ["Extrude area", "Выдавить область"],
    CmdSetExtrusionHeight = ["Set extrusion height", "Задать высоту выдавливания"],
    CmdCreatePath = ["Create path", "Создать путь"],
    CmdReplacePath = ["Replace path", "Заменить путь"],
    CmdCollapseToMesh = ["Collapse to mesh", "Свернуть в сетку"],
    CmdSetBoolean = ["Set boolean", "Задать булеву операцию"],
    CmdSetToolMark = ["Set tool mark", "Пометить как инструмент"],
    FieldTool = ["Tool", "Инструмент"],
    CmdRemoveBoolean = ["Remove boolean", "Удалить булеву операцию"],
    CmdSetEdgeBanding = ["Set edge banding", "Задать кромку"],
    CmdClearEdgeBanding = ["Clear edge banding", "Убрать кромку"],
    CmdSetCornerBanding = ["Set corner banding", "Задать кромку угла"],
    CmdClearCornerBanding = ["Clear corner banding", "Убрать кромку угла"],
    CmdSetContourBanding = ["Set contour banding", "Задать кромку контура"],
    CmdClearContourBanding = ["Clear contour banding", "Убрать кромку контура"],
    CmdSetCornerFeature = ["Set corner feature", "Задать обработку угла"],
    CmdClearCornerFeature = ["Clear corner feature", "Убрать обработку угла"],
    CmdSetEdgeBow = ["Set edge bow", "Задать прогиб кромки"],
    CmdClearEdgeBow = ["Clear edge bow", "Убрать прогиб кромки"],
    CmdSetBevel = ["Set end bevel", "Задать скос торца"],
    CmdClearBevel = ["Clear end bevel", "Убрать скос торца"],
    CmdAddMachining = ["Add machining", "Добавить обработку"],
    CmdUpdateMachining = ["Update machining", "Изменить обработку"],
    CmdRemoveMachining = ["Remove machining", "Удалить обработку"],
    CmdMoveMachining = ["Move machining", "Переместить обработку"],
    CmdCreatePanel = ["Create panel", "Создать панель"],
    CmdCreateContourPanel = ["Create contour panel", "Создать панель по контуру"],
    CmdSetPanelOutline = ["Set panel outline", "Задать контур панели"],
    CmdBakePanelOutline = ["Bake panel outline into a contour", "Преобразовать контур панели в кривую"],
    CmdRenamePanel = ["Rename panel", "Переименовать панель"],
    CmdResizePanel = ["Resize panel", "Изменить размер панели"],
    CmdSetPanelMaterial = ["Set panel material", "Задать материал панели"],
}

impl Message {
    /// "1 point / 2 points / 5 points", a counted message the tests exercise.
    pub const fn test_points(n: usize) -> crate::Counted {
        Self::counted(n, Self::TestPointOne, Self::TestPointFew, Self::TestPointMany)
    }
}
