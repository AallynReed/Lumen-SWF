//! Port of `macromedia/asc/embedding/WarningConstants.java`: the coach warning codes, the English message table
//! and the tables of unsupported ActionScript 2.0 properties, methods and event handlers.

#![allow(non_upper_case_globals)]

pub const kNumWarningConstants: i32 = 521;
pub const kNumPropertyWarnings: i32 = 174;
pub const kNumMethodWarnings: i32 = 218;
pub const kNumEventWarnings: i32 = 81;
pub const kWarning_ConstructorReturnsValue: i32 = 1030;
pub const kWarning_ConstructorReturnsValue_specific: i32 = 1031;
pub const kWarning_BooleanConstructorWithNoArgs: i32 = 1034;
pub const kWarning_BooleanConstructorWithNoArgs_specific: i32 = 1035;
pub const kWarning_NumberFromStringChanges: i32 = 1038;
pub const kWarning_NumberFromStringChanges_specific: i32 = 1039;
pub const kWarning_ArrayToStringChanges: i32 = 1044;
pub const kWarning_ArrayToStringChanges_specific: i32 = 1045;
pub const kWarning_DepricatedPropertyError: i32 = 1058;
pub const kWarning_DepricatedPropertyError_specific: i32 = 1059;
pub const kWarning_DepricatedFunctionError: i32 = 1060;
pub const kWarning_DepricatedFunctionError_specific: i32 = 1061;
pub const kWarning_ChangesInResolve: i32 = 1066;
pub const kWarning_ChangesInResolve_specific: i32 = 1067;
pub const kUnsupportedProp_Highquality: i32 = 2092;
pub const kUnsupportedProp_DisplayObj_HighQuality: i32 = 2093;
pub const kWarning_LevelNotSupported: i32 = 1070;
pub const kWarning_LevelNotSupported_specific: i32 = 1071;
pub const kWarning_ClassIsSealed: i32 = 1072;
pub const kWarning_ClassIsSealed_specific: i32 = 1073;
pub const kUnsupportedProp_version: i32 = 2090;
pub const kUnsupportedProp_Focusrect: i32 = 2091;
pub const kUnsupportedProp_DisplayObj_Quality: i32 = 2094;
pub const kUnsupportedProp_Quality: i32 = 2095;
pub const kUnsupportedProp_TextField_Soundbuftime: i32 = 2096;
pub const kUnsupportedProp_TextField_Target: i32 = 2097;
pub const kWarning_ScopingChangeInThis: i32 = 1082;
pub const kWarning_ScopingChangeInThis_specific: i32 = 1083;
pub const kWarning_MissingNamespaceDecl: i32 = 1084;
pub const kWarning_MissingNamespaceDecl_specific: i32 = 1085;
pub const kWarning_ForVarInChanges: i32 = 1086;
pub const kWarning_ForVarInChanges_specific: i32 = 1087;
pub const kWarning_InternalError: i32 = 1088;
pub const kWarning_InternalError_specific: i32 = 1089;
pub const kWarning_DepricatedEventHandlerError: i32 = 1090;
pub const kWarning_DepricatedEventHandlerError_specific: i32 = 1091;
pub const kWarning_NegativeUintLiteral: i32 = 1092;
pub const kWarning_NegativeUintLiteral_specific: i32 = 1093;
pub const kUnsupportedProp_MovieClip_target: i32 = 2110;
pub const kUnsupportedProp_NewLine: i32 = 2102;
pub const kWarning_BadNullComparision: i32 = 1096;
pub const kWarning_BadNullComparision_specific: i32 = 1097;
pub const kWarning_BadNaNComparision: i32 = 1098;
pub const kWarning_BadNaNComparision_specific: i32 = 1099;
pub const kWarning_AssignmentWithinConditional: i32 = 1100;
pub const kWarning_AssignmentWithinConditional_specific: i32 = 1101;
pub const kWarning_BadNullAssignment: i32 = 1102;
pub const kWarning_BadNullAssignment_specific: i32 = 1103;
pub const kWarning_NoConstructor: i32 = 1104;
pub const kWarning_NoConstructor_specific: i32 = 1105;
pub const kWarning_UnexpectedEmptyStatement: i32 = 1106;
pub const kWarning_UnexpectedEmptyStatement_specific: i32 = 1107;
pub const kUnsupportedProp_Parent: i32 = 2105;
pub const kUnsupportedProp_Root: i32 = 2106;
pub const kWarning_ConstNotInitialized: i32 = 1110;
pub const kWarning_ConstNotInitialized_specific: i32 = 1111;
pub const kWarning_BadArrayCast: i32 = 1112;
pub const kWarning_BadArrayCast_specific: i32 = 1113;
pub const kWarning_NoExplicitSuperCallInConstructor: i32 = 1114;
pub const kWarning_NoExplicitSuperCallInConstructor_specific: i32 = 1115;
pub const kUnsupportedProp_TargetPath: i32 = 2113;
pub const kUnsupportedProp_MaxScroll: i32 = 2103;
pub const kUnsupportedProp_Video_Width: i32 = 2115;
pub const kUnsupportedProp__Proto__: i32 = 2116;
pub const kUnsupportedProp_Stage: i32 = 2117;
pub const kUnsupportedProp__remoteClass: i32 = 2118;
pub const kUnsupportedProp_Button_Target: i32 = 2108;
pub const kUnsupportedProp_MovieClip_Parent: i32 = 2109;
pub const kUnsupportedProp_TextField_Url: i32 = 2098;
pub const kUnsupportedProp_TextField_Variable: i32 = 2099;
pub const kUnsupportedProp_Scroll: i32 = 2112;
pub const kUnsupportedProp_Video_Height: i32 = 2114;
pub const kUnsupportedProp_Level: i32 = 2104;
pub const kUnsupportedProp_Arguments_Caller: i32 = 2107;
pub const kUnsupportedProp_MovieClip_hitArea: i32 = 2111;
pub const kWarning_Event_onStatus: i32 = 3187;
pub const kWarning_Event_onID3: i32 = 3188;
pub const kWarning_Event_onLoad: i32 = 3189;
pub const kWarning_Event_onSoundComplete: i32 = 3190;
pub const kWarning_Event_onSetFocus: i32 = 3191;
pub const kWarning_Event_onResize: i32 = 3192;
pub const kWarning_Event_onChanged: i32 = 3193;
pub const kWarning_Event_onKillFocus: i32 = 3194;
pub const kWarning_Event_onScroller: i32 = 3195;
pub const kWarning_Event_onMouseDown: i32 = 3198;
pub const kWarning_Event_onMouseUp: i32 = 3199;
pub const kWarning_Event_onMouseMove: i32 = 3200;
pub const kWarning_Event_onMouseWheel: i32 = 3201;
pub const kWarning_Event_onKeyDown: i32 = 3202;
pub const kWarning_Event_onKeyUp: i32 = 3203;
pub const kWarning_Event_onData: i32 = 3204;
pub const kWarning_Event_onHTTPStatus: i32 = 3205;
pub const kWarning_Event_onDragOut: i32 = 3206;
pub const kWarning_Event_onDragOver: i32 = 3207;
pub const kWarning_Event_onPress: i32 = 3211;
pub const kWarning_Event_onRelease: i32 = 3212;
pub const kWarning_Event_onReleaseOutside: i32 = 3213;
pub const kWarning_Event_onRollOut: i32 = 3214;
pub const kWarning_Event_onRollOver: i32 = 3215;
pub const kWarning_Event_onActivity: i32 = 3217;
pub const kWarning_Event_onSelect: i32 = 3219;
pub const kWarning_Event_onEnterFrame: i32 = 3226;
pub const kWarning_Event_onUnload: i32 = 3240;
pub const kWarning_Event_onLoadComplete: i32 = 3241;
pub const kWarning_Event_onLoadError: i32 = 3242;
pub const kWarning_Event_onLoadInit: i32 = 3243;
pub const kWarning_Event_onLoadProgress: i32 = 3244;
pub const kWarning_Event_onLoadStart: i32 = 3245;
pub const kWarning_Event_onClose: i32 = 3249;
pub const kWarning_Event_onConnect: i32 = 3250;
pub const kWarning_Event_onXML: i32 = 3252;
pub const kUnsupportedMeth_hasChildNodes: i32 = 3253;
pub const kUnsupportedMeth_XMLEvent: i32 = 3254;
pub const kUnsupportedMeth_XMLDoc: i32 = 3255;
pub const kUnsupportedMeth_Accessibility_isActive: i32 = 3256;
pub const kUnsupportedProp_ActivityEvent_ACTIVITY_TYPE: i32 = 3257;
pub const kUnsupportedMeth_SimpleButton_getDepth: i32 = 3258;
pub const kUnsupportedMeth_SimpleButton_swapDepths: i32 = 3259;
pub const kUnsupportedMeth_SimpleButton_getInstanceAtDepth: i32 = 3260;
pub const kUnsupportedMeth_SimpleButton_getNextHighestDepth: i32 = 3261;
pub const kUnsupportedMeth_ByteArray_available: i32 = 3262;
pub const kUnsupportedMeth_ByteArray_getFilePointer: i32 = 3263;
pub const kUnsupportedMeth_ByteArray_seek: i32 = 3264;
pub const kUnsupportedMeth_Camera_get: i32 = 3265;
pub const kUnsupportedProp_Camera_currentFps: i32 = 3266;
pub const kUnsupportedMeth_Camera_setKeyFrameInterval: i32 = 3267;
pub const kUnsupportedMeth_Camera_setLoopback: i32 = 3268;
pub const kUnsupportedMeth_ColorTransform_getRGB: i32 = 3269;
pub const kUnsupportedMeth_ColorTransform_setRGB: i32 = 3270;
pub const kUnsupportedMeth_DisplayObjectContainer_getTextSnapshot: i32 = 3271;
pub const kUnsupportedMeth_ContextMenu_copy: i32 = 3272;
pub const kUnsupportedProp_ContextMenu_forward_back: i32 = 3273;
pub const kUnsupportedMeth_ContextMenuItem_copy: i32 = 3274;
pub const kUnsupportedMeth_CustomActions_listActions: i32 = 3275;
pub const kUnsupportedProp_DataEvent_DATA_TYPE: i32 = 3276;
pub const kUnsupportedProp_DisplayObject_xscale: i32 = 3277;
pub const kUnsupportedProp_DisplayObject__xscale: i32 = 3278;
pub const kUnsupportedProp_DisplayObject_yscale: i32 = 3279;
pub const kUnsupportedProp_DisplayObject__yscale: i32 = 3280;
pub const kUnsupportedProp_DisplayObject_xmouse: i32 = 3281;
pub const kUnsupportedProp_DisplayObject__xmouse: i32 = 3282;
pub const kUnsupportedProp_DisplayObject_ymouse: i32 = 3283;
pub const kUnsupportedProp_DisplayObject__ymouse: i32 = 3284;
pub const kUnsupportedProp_DisplayObject_setRootClass: i32 = 3285;
pub const kUnsupportedProp_DisplayObject_mapSymbolToClass: i32 = 3286;
pub const kUnsupportedProp_DisplayObject__name: i32 = 3287;
pub const kUnsupportedProp_DisplayObject__parent: i32 = 3288;
pub const kUnsupportedProp_DisplayObject__mask: i32 = 3289;
pub const kUnsupportedProp_DisplayObject__visible: i32 = 3290;
pub const kUnsupportedProp_DisplayObject__x: i32 = 3291;
pub const kUnsupportedProp_DisplayObject__y: i32 = 3292;
pub const kUnsupportedProp_DisplayObject__rotation: i32 = 3293;
pub const kUnsupportedProp_DisplayObject__alpha: i32 = 3294;
pub const kUnsupportedProp_DisplayObject__width: i32 = 3295;
pub const kUnsupportedProp_DisplayObject__height: i32 = 3296;
pub const kUnsupportedMeth_ExternalInterface_available: i32 = 3297;
pub const kUnsupportedProp_ErrorEvent_ERROR_TYPE: i32 = 3298;
pub const kUnsupportedMeth_Event_isDefaultPrevented: i32 = 3299;
pub const kUnsupportedProp_Event_ACTIVATE_TYPE: i32 = 3300;
pub const kUnsupportedProp_Event_ADDED_TYPE: i32 = 3301;
pub const kUnsupportedProp_Event_CANCEL_TYPE: i32 = 3302;
pub const kUnsupportedProp_Event_CHANGE_TYPE: i32 = 3303;
pub const kUnsupportedProp_Event_CLOSE_TYPE: i32 = 3304;
pub const kUnsupportedProp_Event_COMPLETE_TYPE: i32 = 3305;
pub const kUnsupportedProp_Event_CONNECT_TYPE: i32 = 3306;
pub const kUnsupportedProp_Event_DEACTIVATE_TYPE: i32 = 3307;
pub const kUnsupportedProp_Event_ENTER_FRAME_TYPE: i32 = 3308;
pub const kUnsupportedProp_Event_ID3_TYPE: i32 = 3309;
pub const kUnsupportedProp_Event_INIT_TYPE: i32 = 3310;
pub const kUnsupportedProp_Event_MOUSE_LEAVE_TYPE: i32 = 3311;
pub const kUnsupportedProp_Event_OPEN_TYPE: i32 = 3312;
pub const kUnsupportedProp_Event_REMOVED_TYPE: i32 = 3313;
pub const kUnsupportedProp_Event_RENDER_TYPE: i32 = 3314;
pub const kUnsupportedProp_Event_RESIZE_TYPE: i32 = 3315;
pub const kUnsupportedProp_Event_SCROLL_TYPE: i32 = 3316;
pub const kUnsupportedProp_Event_SELECT_TYPE: i32 = 3317;
pub const kUnsupportedProp_Event_SOUND_COMPLETE_TYPE: i32 = 3318;
pub const kUnsupportedProp_Event_TAB_CHILDREN_CHANGE_TYPE: i32 = 3319;
pub const kUnsupportedProp_Event_TAB_ENABLED_CHANGE_TYPE: i32 = 3320;
pub const kUnsupportedProp_Event_TAB_INDEX_CHANGE_TYPE: i32 = 3321;
pub const kUnsupportedProp_Event_UNLOAD_TYPE: i32 = 3322;
pub const kUnsupportedProp_FocusEvent_FOCUS_IN_TYPE: i32 = 3323;
pub const kUnsupportedProp_FocusEvent_FOCUS_OUT_TYPE: i32 = 3324;
pub const kUnsupportedProp_FocusEvent_KEY_FOCUS_CHANGE_TYPE: i32 = 3325;
pub const kUnsupportedProp_FocusEvent_MOUSE_FOCUS_CHANGE_TYPE: i32 = 3326;
pub const kUnsupportedMeth_Graphics_beginImageFill: i32 = 3327;
pub const kUnsupportedProp_BitmapFilter_XYZ_QUALITY: i32 = 3328;
pub const kUnsupportedMeth_Keyboard_getAscii: i32 = 3329;
pub const kUnsupportedMeth_Keyboard_getCode: i32 = 3330;
pub const kUnsupportedMeth_Keyboard_isDown: i32 = 3331;
pub const kUnsupportedMeth_Keyboard_isToggled: i32 = 3332;
pub const kUnsupportedProp_Keyboard_PGDN: i32 = 3333;
pub const kUnsupportedProp_Keyboard_PGUP: i32 = 3334;
pub const kUnsupportedProp_Keyboard_DELETEKEY: i32 = 3335;
pub const kUnsupportedProp_Keyboard_CAPSLOCK: i32 = 3336;
pub const kUnsupportedProp_Keyboard_NUMPAD: i32 = 3337;
pub const kUnsupportedProp_InteractiveObject__focusrect: i32 = 3338;
pub const kUnsupportedProp_InteractiveObject_menu: i32 = 3339;
pub const kUnsupportedProp_KeyboardEvent_ascii: i32 = 3340;
pub const kUnsupportedProp_KeyboardEvent_code: i32 = 3341;
pub const kUnsupportedProp_Loader_loaderInfo: i32 = 3342;
pub const kUnsupportedProp_Loader_preload: i32 = 3343;
pub const kUnsupportedProp_Loader_loadCached: i32 = 3344;
pub const kUnsupportedProp_LoaderInfo_loaderUrl: i32 = 3345;
pub const kUnsupportedMeth_LocalConnection_domain: i32 = 3346;
pub const kUnsupportedProp_ContextMenuEvent_menuOwner: i32 = 3347;
pub const kUnsupportedMeth_Microphone_get: i32 = 3348;
pub const kUnsupportedMeth_MovieClip_attachMovie: i32 = 3349;
pub const kUnsupportedMeth_MovieClip_createEmptyMovieClip: i32 = 3350;
pub const kUnsupportedMeth_MovieClip_createTextField: i32 = 3351;
pub const kUnsupportedMeth_MovieClip_removeMovieClip: i32 = 3352;
pub const kUnsupportedMeth_MovieClip_loadMovie: i32 = 3353;
pub const kUnsupportedMeth_MovieClip_unloadMovie: i32 = 3354;
pub const kUnsupportedMeth_MovieClip_unloadMovieNum: i32 = 3355;
pub const kUnsupportedMeth_MovieClip_getDepth: i32 = 3356;
pub const kUnsupportedMeth_MovieClip_swapDepths: i32 = 3357;
pub const kUnsupportedMeth_MovieClip_getInstanceAtDepth: i32 = 3358;
pub const kUnsupportedMeth_MovieClip_getNextHighestDepth: i32 = 3359;
pub const kUnsupportedMeth_MovieClip_attachAudio: i32 = 3360;
pub const kUnsupportedMeth_MovieClip_getBytesLoaded: i32 = 3361;
pub const kUnsupportedMeth_MovieClip_getBytesTotal: i32 = 3362;
pub const kUnsupportedMeth_MovieClip_getURL: i32 = 3363;
pub const kUnsupportedProp_MovieClip_url: i32 = 3364;
pub const kUnsupportedProp_MovieClip__url: i32 = 3365;
pub const kUnsupportedMeth_MovieClip_setMask: i32 = 3366;
pub const kUnsupportedMeth_MovieClip_getSWFVersion: i32 = 3367;
pub const kUnsupportedProp_MovieClip__currentframe: i32 = 3368;
pub const kUnsupportedProp_MovieClip__framesloaded: i32 = 3369;
pub const kUnsupportedProp_MovieClip__totalframes: i32 = 3370;
pub const kUnsupportedProp_MovieClip_lockroot: i32 = 3371;
pub const kUnsupportedProp_MovieClip__lockroot: i32 = 3372;
pub const kUnsupportedProp_MovieClip__soundbuftime: i32 = 3373;
pub const kUnsupportedMeth_MovieClip_clear: i32 = 3374;
pub const kUnsupportedMeth_MovieClip_beginFill: i32 = 3375;
pub const kUnsupportedMeth_MovieClip_beginGradientFill: i32 = 3376;
pub const kUnsupportedMeth_MovieClip_lineGradientStyle: i32 = 3377;
pub const kUnsupportedMeth_MovieClip_beginImageFill: i32 = 3378;
pub const kUnsupportedMeth_MovieClip_lineStyle: i32 = 3379;
pub const kUnsupportedMeth_MovieClip_drawRect: i32 = 3380;
pub const kUnsupportedMeth_MovieClip_drawRoundRect: i32 = 3381;
pub const kUnsupportedMeth_MovieClip_drawRoundRectComplex: i32 = 3382;
pub const kUnsupportedMeth_MovieClip_drawCircle: i32 = 3383;
pub const kUnsupportedMeth_MovieClip_moveTo: i32 = 3384;
pub const kUnsupportedMeth_MovieClip_lineTo: i32 = 3385;
pub const kUnsupportedMeth_MovieClip_curveTo: i32 = 3386;
pub const kUnsupportedMeth_MovieClip_endFill: i32 = 3387;
pub const kUnsupportedMeth_NetStream_setBufferTime: i32 = 3388;
pub const kUnsupportedProp_NetStream_currentFps: i32 = 3389;
pub const kUnsupportedProp_NetStream_videocodec: i32 = 3390;
pub const kUnsupportedProp_NetStream_audiocodec: i32 = 3391;
pub const kUnsupportedMeth_ProductManager_isIntalled: i32 = 3392;
pub const kUnsupportedMeth_ProductManager_installedVersion: i32 = 3393;
pub const kUnsupportedMeth_ProductManager_isRunning: i32 = 3394;
pub const kUnsupportedMeth_Point_addTo: i32 = 3395;
pub const kUnsupportedProp_Proxy_delDescendants: i32 = 3396;
pub const kUnsupportedMeth_Profiler_heapdump: i32 = 3397;
pub const kUnsupportedProp_ProgressEvent_current: i32 = 3398;
pub const kUnsupportedProp_ProgressEvent_total: i32 = 3399;
pub const kUnsupportedMeth_Rectangle_isEmpty: i32 = 3400;
pub const kUnsupportedMeth_SoundTransform_setPan: i32 = 3401;
pub const kUnsupportedMeth_Socket_available: i32 = 3402;
pub const kUnsupportedMeth_SharedObject_getSize: i32 = 3403;
pub const kUnsupportedMeth_SharedObject_setFps: i32 = 3404;
pub const kUnsupportedMeth_Sprite_getSWF: i32 = 3405;
pub const kUnsupportedMeth_Sprite_constructChild: i32 = 3406;
pub const kUnsupportedProp_Sprite__droptarget: i32 = 3407;
pub const kUnsupportedMeth_Stage_getFocus: i32 = 3408;
pub const kUnsupportedMeth_Stage_setFocus: i32 = 3409;
pub const kUnsupportedProp_Stage_showMenu: i32 = 3411;
pub const kUnsupportedMeth_StyleSheet_getStyleNames: i32 = 3412;
pub const kUnsupportedMeth_StyleSheet_onData: i32 = 3413;
pub const kUnsupportedMeth_StyleSheet_load: i32 = 3414;
pub const kUnsupportedProp_StyleSheet_loaded: i32 = 3415;
pub const kUnsupportedMeth_StyleSheet_getBytesLoaded: i32 = 3416;
pub const kUnsupportedMeth_StyleSheet_getBytesTotal: i32 = 3417;
pub const kUnsupportedMeth_IME_getEnabled: i32 = 3418;
pub const kUnsupportedMeth_IME_setEnabled: i32 = 3419;
pub const kUnsupportedMeth_IME_getInstance: i32 = 3420;
pub const kUnsupportedMeth_IME_getConversionMode: i32 = 3421;
pub const kUnsupportedMeth_IME_setConversionMode: i32 = 3422;
pub const kUnsupportedProp_System_getAvmplusVersion: i32 = 3423;
pub const kUnsupportedProp_SWFLoaderInfo_SWFVersion: i32 = 3424;
pub const kUnsupportedProp_SWFLoaderInfo_ASVersion: i32 = 3425;
pub const kUnsupportedMeth_TextField_getNewTextFormat: i32 = 3426;
pub const kUnsupportedMeth_TextField_setNewTextFormat: i32 = 3427;
pub const kUnsupportedMeth_TextField_getDepth: i32 = 3428;
pub const kUnsupportedMeth_TextField_swapDepths: i32 = 3429;
pub const kUnsupportedMeth_TextField_getInstanceAtDepth: i32 = 3430;
pub const kUnsupportedMeth_TextField_getNextHighestDepth: i32 = 3431;
pub const kUnsupportedMeth_TextField_replaceSel: i32 = 3432;
pub const kUnsupportedMeth_TextField_getLineIndexOfCharacter: i32 = 3433;
pub const kUnsupportedMeth_TextField_getSelectionBeginIndex: i32 = 3434;
pub const kUnsupportedMeth_TextField_getSelectionEndIndex: i32 = 3435;
pub const kUnsupportedMeth_TextField_getCaretIndex: i32 = 3436;
pub const kUnsupportedMeth_TextField_getFontList: i32 = 3437;
pub const kUnsupportedProp_TextField_maxscroll: i32 = 3438;
pub const kUnsupportedProp_TextField_hscroll: i32 = 3439;
pub const kUnsupportedProp_TextField_maxhscroll: i32 = 3440;
pub const kUnsupportedProp_TextField_newTextFormat: i32 = 3441;
pub const kUnsupportedMeth_TextFormat_getTextExtent: i32 = 3442;
pub const kUnsupportedMeth_TextSnapshot_getCount: i32 = 3443;
pub const kUnsupportedProp_URLLoader_navigate: i32 = 3444;
pub const kUnsupportedMeth_URLLoader_send: i32 = 3445;
pub const kUnsupportedProp_URLLoader_binary: i32 = 3446;
pub const kUnsupportedMeth_URLStream_available: i32 = 3447;
pub const kUnsupportedProp_URLRequest_digest: i32 = 3448;
pub const kUnsupportedProp_URLRequest_importToSandbox: i32 = 3449;
pub const kUnsupportedMeth_URLRequest_addRequestHeader: i32 = 3450;
pub const kUnsupportedMeth_XMLDocument_load: i32 = 3451;
pub const kUnsupportedMeth_XMLDocument_send: i32 = 3452;
pub const kUnsupportedMeth_XMLDocument_sendAndLoad: i32 = 3453;
pub const kUnsupportedMeth_XMLDocument_onData: i32 = 3454;
pub const kUnsupportedMeth_XMLDocument_addRequestHeader: i32 = 3455;
pub const kUnsupportedMeth_XMLDocument_getBytesLoaded: i32 = 3456;
pub const kUnsupportedMeth_XMLDocument_getBytesTotal: i32 = 3457;
pub const kUnsupportedProp_XMLDocument_loaded: i32 = 3458;
pub const kUnsupportedProp_XMLDocument_contentType: i32 = 3459;
pub const kUnsupportedProp_XMLDocument_status: i32 = 3460;
pub const kUnsupportedProp_Button: i32 = 3461;
pub const kUnsupportedProp_Container: i32 = 3462;
pub const kUnsupportedProp_Image: i32 = 3463;
pub const kUnsupportedProp_ImageFilter: i32 = 3464;
pub const kUnsupportedProp_ImageSprite: i32 = 3465;
pub const kUnsupportedProp_ImageLoaderInfo: i32 = 3466;
pub const kUnsupportedProp_ImeEvent: i32 = 3467;
pub const kUnsupportedProp_Key: i32 = 3468;
pub const kUnsupportedProp_LineMetrics: i32 = 3469;
pub const kUnsupportedProp_LoadVars: i32 = 3470;
pub const kUnsupportedProp_MenuEvent: i32 = 3471;
pub const kUnsupportedProp_SystemCapabilities: i32 = 3472;
pub const kUnsupportedProp_TextExtents: i32 = 3473;
pub const kUnsupportedMeth_Button: i32 = 3475;
pub const kUnsupportedMeth_Container: i32 = 3476;
pub const kUnsupportedMeth_Image: i32 = 3477;
pub const kUnsupportedMeth_ImageFilter: i32 = 3478;
pub const kUnsupportedMeth_ImageSprite: i32 = 3479;
pub const kUnsupportedMeth_ImageLoaderInfo: i32 = 3480;
pub const kUnsupportedMeth_ImeEvent: i32 = 3481;
pub const kUnsupportedMeth_Key: i32 = 3482;
pub const kUnsupportedMeth_LineMetrics: i32 = 3483;
pub const kUnsupportedMeth_LoadVars: i32 = 3484;
pub const kUnsupportedMeth_MenuEvent: i32 = 3485;
pub const kUnsupportedMeth_SystemCapabilities: i32 = 3486;
pub const kUnsupportedMeth_TextExtents: i32 = 3487;
pub const kUnsupportedProp_Object___resolve: i32 = 3488;
pub const kUnsupportedMeth_XMLUI_get: i32 = 3489;
pub const kUnsupportedMeth_XMLUI_set: i32 = 3490;
pub const kUnsupportedProp_DisplayObject_accProps: i32 = 3491;
pub const kUnsupportedMeth_DisplayObject_setScalingGrid: i32 = 3492;
pub const kUnsupportedMeth_Graphics_drawCircle: i32 = 3493;
pub const kUnsupportedProp_NetConnection_isConnected: i32 = 3494;
pub const kUnsupportedProp_Socket_isConnected: i32 = 3495;
pub const kUnsupportedProp_URLStream_isConnected: i32 = 3496;
pub const kUnsupportedProp_SyncEvent_list: i32 = 3497;
pub const kUnsupportedProp_TextField_scroll: i32 = 3498;
pub const kUnsupportedProp_TextField_bottomScroll: i32 = 3499;
pub const kUnsupportedProp_BitmapData_RED_CHANNEL: i32 = 3500;
pub const kUnsupportedProp_BitmapData_GREEN_CHANNEL: i32 = 3501;
pub const kUnsupportedProp_BitmapData_BLUE_CHANNEL: i32 = 3502;
pub const kUnsupportedProp_BitmapData_ALPHA_CHANNEL: i32 = 3503;
pub const kUnsupportedMeth_instanceof: i32 = 3504;
pub const kUnsupportedMeth_System_showSettings: i32 = 3505;
pub const kUnsupportedProp_System_useCodepage: i32 = 3506;
pub const kUnsupportedProp_AsBroadcaster: i32 = 3507;
pub const kUnsupportedProp_SimpleButton_Soundbuftime: i32 = 3508;
pub const kUnsupportedMeth_BitmapData_loadBitmap: i32 = 3509;
pub const kUnsupportedMeth_MovieClip_loadVariables: i32 = 3510;
pub const kUnsupportedProp_MovieClipLoader: i32 = 3511;
pub const kUnsupportedMeth_MovieClipLoader: i32 = 3512;
pub const kUnsupportedMeth_IME_addListener: i32 = 3513;
pub const kUnsupportedMeth_IME_removeListener: i32 = 3514;
pub const kUnsupportedProp_IME_ALPHANUMERIC_FULL: i32 = 3515;
pub const kUnsupportedProp_IME_ALPHANUMERIC_HALF: i32 = 3516;
pub const kUnsupportedProp_IME_CHINESE: i32 = 3517;
pub const kUnsupportedProp_IME_JAPANESE_HIRAGANA: i32 = 3518;
pub const kUnsupportedProp_IME_JAPANESE_KATAKANA_FULL: i32 = 3519;
pub const kUnsupportedProp_IME_JAPANESE_KATAKANA_HALF: i32 = 3520;
pub const kUnsupportedProp_IME_KOREAN: i32 = 3521;
pub const kUnsupportedProp_IME_UNKNOWN: i32 = 3522;
pub const kUnsupportedMeth_FileReferenceList_addListener: i32 = 3523;
pub const kUnsupportedMeth_FileReferenceList_removeListener: i32 = 3524;
pub const kWarning_Event_onCancel: i32 = 3527;
pub const kUnsupportedProp_Keyboard__listeners: i32 = 3528;
pub const kWarning_Event_onIMEComposition: i32 = 3529;
pub const kUnsupportedProp_SimpleButton__url: i32 = 3530;
pub const kUnsupportedMeth_Date_getYear: i32 = 3531;
pub const kUnsupportedMeth_Date_setYear: i32 = 3532;
pub const kUnsupportedMeth_Date_getUTCYear: i32 = 3533;
pub const kUnsupportedMeth_Microphone_setRate: i32 = 3534;
pub const kUnsupportedProp_Selection: i32 = 3535;
pub const kUnsupportedMeth_Selection: i32 = 3536;
pub const kUnsupportedMeth_Microphone_setGain: i32 = 3537;
pub const kUnsupportedMeth_ColorTransform_getTransform: i32 = 3538;
pub const kUnsupportedMeth_ColorTransform_setTransform: i32 = 3539;
pub const kUnsupportedProp_MovieClip_focusEnabled: i32 = 3540;
pub const kUnsupportedMeth_MovieClip_beginBitmapFill: i32 = 3541;
pub const kUnsupportedMeth_MovieClip_hitTest: i32 = 3542;
pub const kUnsupportedMeth_MovieClip_attachBitmap: i32 = 3543;
pub const kUnsupportedMeth_Sound_loadSound: i32 = 3544;
pub const kUnsupportedMeth_Sound_getVolume: i32 = 3545;
pub const kUnsupportedMeth_Sound_getTransform: i32 = 3546;
pub const kUnsupportedMeth_Sound_getPan: i32 = 3547;
pub const kUnsupportedMeth_Sound_setPan: i32 = 3548;
pub const kUnsupportedMeth_Sound_getBytesLoaded: i32 = 3549;
pub const kUnsupportedMeth_Sound_getBytesTotal: i32 = 3550;
pub const kWarning_SlowTextFieldAddition: i32 = 3551;
pub const kWarning_SlowTextFieldAddition_specific: i32 = 3552;
pub const kWarning_UnlikelyFunctionValue: i32 = 3553;
pub const kWarning_UnlikelyFunctionValue_specific: i32 = 3554;
pub const kWarning_InstanceOfChanges: i32 = 3555;
pub const kWarning_InstanceOfChanges_specific: i32 = 3556;
pub const kUnsupportedProp_LocalConnection_allowDomain: i32 = 3557;
pub const kUnsupportedProp_LocalConnection_allowInsecureDomain: i32 = 3558;
pub const kUnsupportedMeth_call: i32 = 3559;
pub const kUnsupportedProp_Color: i32 = 3560;
pub const kUnsupportedMeth_Color: i32 = 3561;
pub const kUnsupportedProp_System_exactSettings: i32 = 3562;
pub const kUnsupportedProp_capabilities: i32 = 3563;
pub const kUnsupportedMeth_capabilities: i32 = 3564;
pub const kUnsupportedMeth_FileReference_addListener: i32 = 3565;
pub const kUnsupportedMeth_FileReference_removeListener: i32 = 3566;
pub const kWarning_Event_onComplete: i32 = 3567;
pub const kWarning_Event_onHTTPError: i32 = 3568;
pub const kWarning_Event_onIOError: i32 = 3569;
pub const kWarning_Event_onProgress: i32 = 3570;
pub const kWarning_Event_onSecurityError: i32 = 3571;
pub const kWarning_Event_onOpen: i32 = 3572;
pub const kWarning_XML_ClassHasChanged: i32 = 3573;
pub const kWarning_XML_ClassHasChanged_specific: i32 = 3574;
pub const kWarning_BadDateCast: i32 = 3575;
pub const kWarning_BadDateCast_specific: i32 = 3576;
pub const kWarning_ImportHidesClass: i32 = 3581;
pub const kWarning_ImportHidesClass_specific: i32 = 3582;
pub const kWarning_DuplicateArgumentNames: i32 = 3583;
pub const kWarning_DuplicateArgumentNames_specific: i32 = 3584;
pub const kUnsupportedMeth_Rectangle_containsRectangle: i32 = 3585;
pub const kUnsupportedMeth_eval: i32 = 3586;
pub const kUnsupportedMeth_getVersion: i32 = 3587;
pub const kUnsupportedMeth_ifFrameLoaded: i32 = 3588;
pub const kUnsupportedProp__global: i32 = 3589;
pub const kWarning_BadBoolAssignment: i32 = 3590;
pub const kWarning_BadBoolAssignment_specific: i32 = 3591;
pub const kWarning_BadES3TypeProp: i32 = 3592;
pub const kWarning_BadES3TypeProp_specific: i32 = 3593;
pub const kWarning_BadES3TypeMethod: i32 = 3594;
pub const kWarning_BadES3TypeMethodProp_specific: i32 = 3595;
pub const kWarning_DuplicateVariableDef: i32 = 3596;
pub const kWarning_DuplicateVariableDef_specific: i32 = 3597;
pub const kWarning_DefinitionShadowedByPackageName: i32 = 3598;
pub const kWarning_DefinitionShadowedByPackageName_specific: i32 = 3599;
pub const kWarning_DeleteOfFixedProperty: i32 = 3600;
pub const kWarning_DeleteOfFixedProperty_specific: i32 = 3601;
pub const kWarning_Deprecated: i32 = 3602;
pub const kWarning_Deprecated_specific: i32 = 3603;
pub const kWarning_DeprecatedMessage: i32 = 3604;
pub const kWarning_DeprecatedMessage_specific: i32 = 3605;
pub const kWarning_DeprecatedUseReplacement: i32 = 3606;
pub const kWarning_DeprecatedUseReplacement_specific: i32 = 3607;
pub const kWarning_DeprecatedSince: i32 = 3608;
pub const kWarning_DeprecatedSince_specific: i32 = 3609;
pub const kWarning_DeprecatedSinceNoReplacement: i32 = 3610;
pub const kWarning_DeprecatedSinceNoReplacement_specific: i32 = 3611;
pub const kUnsupportedMeth_random: i32 = 2617;
pub const kUnsupportedMeth_chr: i32 = 2618;
pub const kUnsupportedMeth_mbchr: i32 = 2619;
pub const kUnsupportedMeth_ord: i32 = 2621;
pub const kUnsupportedMeth_mbord: i32 = 2622;
pub const kUnsupportedMeth_substring: i32 = 2623;
pub const kUnsupportedMeth_mbsubstring: i32 = 2624;
pub const kUnsupportedMeth_length: i32 = 2625;
pub const kUnsupportedMeth_mblength: i32 = 2626;
pub const kUnsupportedMeth_ASNative: i32 = 2627;
pub const kUnsupportedMeth_addProperty: i32 = 2628;
pub const kUnsupportedMeth_getProperty: i32 = 2629;
pub const kUnsupportedMeth_setProperty: i32 = 2630;
pub const kUnsupportedMeth_asfunction: i32 = 2631;
pub const kUnsupportedMeth_clearInterval: i32 = 2633;
pub const kUnsupportedMeth_duplicateMovieClip: i32 = 2634;
pub const kUnsupportedMeth_fscommand: i32 = 2636;
pub const kUnsupportedMeth_getURL: i32 = 2638;
pub const kUnsupportedMeth_gotoAndPlay: i32 = 2639;
pub const kUnsupportedMeth_gotoAndStop: i32 = 2640;
pub const kUnsupportedMeth_play: i32 = 2641;
pub const kUnsupportedMeth_print: i32 = 2642;
pub const kUnsupportedMeth_printAsBitmap: i32 = 2643;
pub const kUnsupportedMeth_printAsBitmapNum: i32 = 2644;
pub const kUnsupportedMeth_printNum: i32 = 2645;
pub const kUnsupportedMeth_removeMovieClip: i32 = 2646;
pub const kUnsupportedMeth_setInterval: i32 = 2647;
pub const kUnsupportedMeth_nextFrame: i32 = 2648;
pub const kUnsupportedMeth_startDrag: i32 = 2649;
pub const kUnsupportedMeth_stop: i32 = 2650;
pub const kUnsupportedMeth_stopAllSounds: i32 = 2651;
pub const kUnsupportedMeth_stopDrag: i32 = 2652;
pub const kUnsupportedMeth_tellTarget: i32 = 2653;
pub const kUnsupportedMeth_toggleHighQuality: i32 = 2654;
pub const kUnsupportedMeth_unloadMovie: i32 = 2656;
pub const kUnsupportedMeth_unloadMovieNum: i32 = 2657;
pub const kUnsupportedMeth_updateAfterEvent: i32 = 2658;
pub const kUnsupportedMeth_Video_attachVideo: i32 = 2659;
pub const kUnsupportedMeth_TextField_StyleSheet_load: i32 = 2660;
pub const kUnsupportedMeth_Object_registerClass: i32 = 2663;
pub const kUnsupportedMeth_Object_unwatch: i32 = 2664;
pub const kUnsupportedMeth_Object_watch: i32 = 2665;
pub const kUnsupportedMeth_loadMovie: i32 = 2666;
pub const kUnsupportedMeth_loadMovieNum: i32 = 2667;
pub const kUnsupportedMeth_loadVariables: i32 = 2668;
pub const kUnsupportedMeth_loadVariablesNum: i32 = 2669;
pub const kUnsupportedMeth_Stage_addListener: i32 = 2678;
pub const kUnsupportedMeth_Stage_removeListener: i32 = 2679;
pub const kUnsupportedMeth_TextField_addListener: i32 = 2680;
pub const kUnsupportedMeth_TextField_removeListener: i32 = 2681;
pub const kUnsupportedMeth_Mouse_addListener: i32 = 2682;
pub const kUnsupportedMeth_Mouse_removeListener: i32 = 2683;
pub const kUnsupportedMeth_Keyboard_addListener: i32 = 2684;
pub const kUnsupportedMeth_Keyboard_removeListener: i32 = 2685;
pub const kUnsupportedMeth_Sound_attachSound: i32 = 2686;
pub const kWarning_NoTypeDecl: i32 = 1008;
pub const kWarning_NoTypeDecl_specific: i32 = 1009;
pub const kWarning_BadUndefinedComparision: i32 = 1012;
pub const kWarning_BadUndefinedComparision_specific: i32 = 1013;

pub const kVoidType: i32 = 0;
pub const kObjectType: i32 = 1;
pub const kFunctionType: i32 = 2;
pub const kStringType: i32 = 3;
pub const kNumberType: i32 = 4;
pub const kBooleanType: i32 = 5;
pub const kArrayType: i32 = 6;
pub const kDateType: i32 = 7;
pub const kMathType: i32 = 8;
pub const kErrorType: i32 = 9;
pub const kRegExpType: i32 = 10;
pub const kDisplayObjectType: i32 = 11;
pub const kMovieClipType: i32 = 12;
pub const kTextFieldType: i32 = 13;
pub const kTextFormatType: i32 = 14;
pub const kMicrophoneType: i32 = 15;
pub const kSimpleButtonType: i32 = 16;
pub const kVideoType: i32 = 17;
pub const kStyleSheetType: i32 = 18;
pub const kSelectionType: i32 = 19;
pub const kColorType: i32 = 20;
pub const kStageType: i32 = 21;
pub const kMouseType: i32 = 22;
pub const kKeyboardType: i32 = 23;
pub const kSoundType: i32 = 24;
pub const kSystemType: i32 = 25;
pub const kXMLType: i32 = 26;
pub const kXMLSocketType: i32 = 27;
pub const kXMLListType: i32 = 28;
pub const kQNameType: i32 = 29;
pub const kLoadVarsType: i32 = 30;
pub const kCameraType: i32 = 31;
pub const kContextMenuType: i32 = 32;
pub const kContextMenuItemType: i32 = 33;
pub const kMovieClipLoaderType: i32 = 34;
pub const kNetStreamType: i32 = 35;
pub const kAccessibilityType: i32 = 36;
pub const kActivityEventType: i32 = 37;
pub const kByteArrayType: i32 = 38;
pub const kColorTransformType: i32 = 39;
pub const kDisplayObjectContainerType: i32 = 40;
pub const kCustomActionsType: i32 = 41;
pub const kDataEventType: i32 = 42;
pub const kExternalInterfaceType: i32 = 43;
pub const kErrorEventType: i32 = 44;
pub const kEventType: i32 = 45;
pub const kFocusEventType: i32 = 46;
pub const kGraphicsType: i32 = 47;
pub const kBitmapFilterType: i32 = 48;
pub const kInteractiveObjectType: i32 = 49;
pub const kKeyboardEventType: i32 = 50;
pub const kLoaderType: i32 = 51;
pub const kLoaderInfoType: i32 = 52;
pub const kLocalConnectionType: i32 = 53;
pub const kContextMenuEventType: i32 = 54;
pub const kProductManagerType: i32 = 55;
pub const kPointType: i32 = 56;
pub const kProxyType: i32 = 57;
pub const kProfilerType: i32 = 58;
pub const kProgressEventType: i32 = 59;
pub const kRectangleType: i32 = 60;
pub const kSoundTransformType: i32 = 61;
pub const kSocketType: i32 = 62;
pub const kSharedObjectType: i32 = 63;
pub const kSpriteType: i32 = 64;
pub const kIMEType: i32 = 65;
pub const kSWFLoaderInfoType: i32 = 66;
pub const kTextSnapshotType: i32 = 67;
pub const kURLLoaderType: i32 = 68;
pub const kURLStreamType: i32 = 69;
pub const kURLRequestType: i32 = 70;
pub const kXMLDocumentType: i32 = 71;
pub const kXMLNodeType: i32 = 72;
pub const kNetConnectionType: i32 = 73;
pub const kSyncEventType: i32 = 74;
pub const kBitmapDataType: i32 = 75;
pub const kXMLUIType: i32 = 76;
pub const kFileReferenceListType: i32 = 77;
pub const kFileReferenceType: i32 = 78;
pub const kNumDefaultTypes: i32 = 79;

pub struct AscWarning {
    pub code: i32,
    pub p_warning: &'static str,
}

pub struct AscWarningInstance {
    pub code: i32,
    pub base_type: i32,
    pub is_static: bool,
    pub name: &'static str,
}

pub static warningConstantsEN: [AscWarning; kNumWarningConstants as usize] = [
    AscWarning { code: kWarning_NoTypeDecl, p_warning: "Missing type declaration." },
    AscWarning { code: kWarning_NoTypeDecl_specific, p_warning: "%s '%s' has no type declaration." },
    AscWarning { code: kWarning_BadUndefinedComparision, p_warning: "Illogical comparison with undefined.  Only untyped variables (or variables of type *) can be undefined." },
    AscWarning { code: kWarning_BadUndefinedComparision_specific, p_warning: "Variables of type %s cannot be undefined. The value undefined will be type coerced to %s before comparison." },
    AscWarning { code: kWarning_ConstructorReturnsValue, p_warning: "Function used in new expression returns a value.  Result will be what the function returns, rather than a new instance of that function." },
    AscWarning { code: kWarning_ConstructorReturnsValue_specific, p_warning: "Migration issue: Result of new %s will be the return value of %s, rather than a new instance of that function." },
    AscWarning { code: kWarning_BooleanConstructorWithNoArgs, p_warning: "Boolean() with no arguments returns false in ActionScript 3.0.  Boolean() returned undefined in ActionScript 2.0." },
    AscWarning { code: kWarning_BooleanConstructorWithNoArgs_specific, p_warning: "Use of Boolean() with no arguments." },
    AscWarning { code: kWarning_NumberFromStringChanges, p_warning: "In ActionScript 3.0, white space is ignored and '' returns 0. Number() returns NaN in ActionScript 2.0 when the parameter is '' or contains white space." },
    AscWarning { code: kWarning_NumberFromStringChanges_specific, p_warning: "Migration issue: When the Number('') function is called with an empty string argument it returns 0 in ActionScript 3.0, and NaN in ActionScript 2.0." },
    AscWarning { code: kWarning_ArrayToStringChanges, p_warning: "Array.toString() format has changed." },
    AscWarning { code: kWarning_ArrayToStringChanges_specific, p_warning: "Migration issue: Array.toString() handling of null and undefined elements has changed." },
    AscWarning { code: kWarning_DepricatedPropertyError, p_warning: "Unsupported ActionScript 2.0 property." },
    AscWarning { code: kWarning_DepricatedPropertyError_specific, p_warning: "Migration issue: The property %s is no longer supported.  %s." },
    AscWarning { code: kWarning_DepricatedFunctionError, p_warning: "Unsupported ActionScript 2.0 function." },
    AscWarning { code: kWarning_DepricatedFunctionError_specific, p_warning: "Migration issue: The method %s is no longer supported.  %s." },
    AscWarning { code: kWarning_ChangesInResolve, p_warning: "__resolve is no longer supported." },
    AscWarning { code: kWarning_ChangesInResolve_specific, p_warning: "Migration issue: __resolve is no longer supported.  Use the new Proxy class for similar functionality." },
    AscWarning { code: kWarning_LevelNotSupported, p_warning: "_level is no longer supported. For more information, see the flash.display package." },
    AscWarning { code: kWarning_LevelNotSupported_specific, p_warning: "Migration issue: _level is no longer supported. For more information, see the flash.display package." },
    AscWarning { code: kWarning_ClassIsSealed, p_warning: "Class is sealed.  It cannot have members added to it dynamically." },
    AscWarning { code: kWarning_ClassIsSealed_specific, p_warning: "Migration issue: %s is not a dynamic class.  Instances cannot have members added to them dynamically." },
    AscWarning { code: kWarning_ScopingChangeInThis, p_warning: "Change in scoping for the this keyword.  Class methods extracted from an instance of a class will always resolve this back to that instance.  In ActionScript 2.0 this is looked up dynamically based on where the method is invoked from." },
    AscWarning { code: kWarning_ScopingChangeInThis_specific, p_warning: "Migration issue: Method %s will behave differently in ActionScript 3.0 due to the change in scoping for the this keyword. See the entry for warning 1083 for additional information." },
    AscWarning { code: kWarning_MissingNamespaceDecl, p_warning: "Missing namespace declaration (e.g. variable is not defined to be public, private, etc.)." },
    AscWarning { code: kWarning_MissingNamespaceDecl_specific, p_warning: "%s will be scoped to the default namespace: %s internal.  It will not be visible outside of this package." },
    AscWarning { code: kWarning_ForVarInChanges, p_warning: "ActionScript 3.0 iterates over an object's properties within a \"for x in target\" statement in random order." },
    AscWarning { code: kWarning_ForVarInChanges_specific, p_warning: "Migration issue: ActionScript 3.0 iterates over an object's properties within a \"for x in target\" statement in random order." },
    AscWarning { code: kWarning_InternalError, p_warning: "Internal error in compiler." },
    AscWarning { code: kWarning_InternalError_specific, p_warning: "Error code: %s." },
    AscWarning { code: kWarning_DepricatedEventHandlerError, p_warning: "EventHandler was not added as a listener." },
    AscWarning { code: kWarning_DepricatedEventHandlerError_specific, p_warning: "Migration issue: %s" },
    AscWarning { code: kWarning_NegativeUintLiteral, p_warning: "Negative value will become a large positive value when assigned to a uint data type." },
    AscWarning { code: kWarning_NegativeUintLiteral_specific, p_warning: "Negative value used where a uint (non-negative) value is expected." },
    AscWarning { code: kWarning_BadNullComparision, p_warning: "Illogical comparison with null." },
    AscWarning { code: kWarning_BadNullComparision_specific, p_warning: "Illogical comparison with null.  Variables of type %s cannot be null." },
    AscWarning { code: kWarning_BadNaNComparision, p_warning: "Illogical comparison with NaN. Any comparison operation involving NaN will evaluate to false because NaN != NaN." },
    AscWarning { code: kWarning_BadNaNComparision_specific, p_warning: "Illogical comparison with NaN.  This statement always evaluates to false." },
    AscWarning { code: kWarning_AssignmentWithinConditional, p_warning: "Assignment within conditional." },
    AscWarning { code: kWarning_AssignmentWithinConditional_specific, p_warning: "Assignment within conditional.  Did you mean == instead of =?" },
    AscWarning { code: kWarning_BadNullAssignment, p_warning: "Impossible null assignment." },
    AscWarning { code: kWarning_BadNullAssignment_specific, p_warning: "null used where a %s value was expected." },
    AscWarning { code: kWarning_NoConstructor, p_warning: "Missing constructor." },
    AscWarning { code: kWarning_NoConstructor_specific, p_warning: "No constructor function was specified for class %s." },
    AscWarning { code: kWarning_UnexpectedEmptyStatement, p_warning: "Empty statement." },
    AscWarning { code: kWarning_UnexpectedEmptyStatement_specific, p_warning: "Empty statement found where block of code expected. Did you type ';' accidentally?" },
    AscWarning { code: kWarning_ConstNotInitialized, p_warning: "Constant not initialized." },
    AscWarning { code: kWarning_ConstNotInitialized_specific, p_warning: "The constant was not initialized." },
    AscWarning { code: kWarning_BadArrayCast, p_warning: "Possibly invalid Array cast operation." },
    AscWarning { code: kWarning_BadArrayCast_specific, p_warning: "Array(x) behaves the same as new Array(x).  To cast a value to type Array use the expression x as Array instead of Array(x)." },
    AscWarning { code: kWarning_NoExplicitSuperCallInConstructor, p_warning: "The super() statement was not called within the constructor." },
    AscWarning { code: kWarning_NoExplicitSuperCallInConstructor_specific, p_warning: "The super() statement will be executed prior to entering this constructor.  Add a call to super() within the constructor if you want to explicitly control when it is executed." },
    AscWarning { code: kUnsupportedProp_version, p_warning: "Use Capabilities.version instead." },
    AscWarning { code: kUnsupportedProp_Focusrect, p_warning: "For more information, see InteractiveObject.focusRect." },
    AscWarning { code: kUnsupportedProp_Highquality, p_warning: "For more information, see Stage.quality." },
    AscWarning { code: kUnsupportedProp_DisplayObj_HighQuality, p_warning: "For more information, see Stage.quality." },
    AscWarning { code: kUnsupportedProp_DisplayObj_Quality, p_warning: "For more information, see Stage.quality." },
    AscWarning { code: kUnsupportedProp_Quality, p_warning: "For more information, see Stage.quality." },
    AscWarning { code: kUnsupportedProp_TextField_Soundbuftime, p_warning: "Use the static property flash.media.SoundMixer.bufferTime instead." },
    AscWarning { code: kUnsupportedProp_TextField_Target, p_warning: "This functionality is no longer supported." },
    AscWarning { code: kUnsupportedProp_TextField_Url, p_warning: "For more information, see LoaderInfo.url." },
    AscWarning { code: kUnsupportedProp_TextField_Variable, p_warning: "This functionality is no longer supported." },
    AscWarning { code: kUnsupportedProp_NewLine, p_warning: "Use '\n' for newline." },
    AscWarning { code: kUnsupportedProp_MaxScroll, p_warning: "For more information, see textField.maxScroll." },
    AscWarning { code: kUnsupportedProp_Level, p_warning: "The concept of levels does not exist in ActionScript 3.0, which instead provides direct access to the display list. See the flash.display package for details." },
    AscWarning { code: kUnsupportedProp_Parent, p_warning: "Use the parent property instead." },
    AscWarning { code: kUnsupportedProp_Root, p_warning: "This property has been removed. The closest equivalent is the Stage, which serves as the root of the ActionScript 3.0 display list." },
    AscWarning { code: kUnsupportedProp_Arguments_Caller, p_warning: "Try declaring caller as an argument of the function instead." },
    AscWarning { code: kUnsupportedProp_Button_Target, p_warning: "This functionality is no longer supported." },
    AscWarning { code: kUnsupportedProp_MovieClip_Parent, p_warning: "Use the parent property instead." },
    AscWarning { code: kUnsupportedProp_MovieClip_target, p_warning: "This functionality is no longer supported." },
    AscWarning { code: kUnsupportedProp_MovieClip_hitArea, p_warning: "For more information, see Sprite.hitArea." },
    AscWarning { code: kUnsupportedProp_Scroll, p_warning: "For more information, see the scrollH and scrollV properties of flash.text.TextField class." },
    AscWarning { code: kUnsupportedProp_TargetPath, p_warning: "Use MovieClip objects directly as arguments instead of paths." },
    AscWarning { code: kUnsupportedProp_Video_Height, p_warning: "For more information, see Video.videoHeight." },
    AscWarning { code: kUnsupportedProp_Video_Width, p_warning: "For more information, see Video.videoWidth." },
    AscWarning { code: kUnsupportedProp__Proto__, p_warning: "For more information, see obsolete  __proto__." },
    AscWarning { code: kUnsupportedProp_Stage, p_warning: "For more information, see DisplayObject.stage." },
    AscWarning { code: kUnsupportedProp__remoteClass, p_warning: "Use the registerClass() method in the flash.net package instead." },
    AscWarning { code: kUnsupportedMeth_random, p_warning: "Use Math.random() instead." },
    AscWarning { code: kUnsupportedMeth_chr, p_warning: "Use String.fromCharCode() instead." },
    AscWarning { code: kUnsupportedMeth_mbchr, p_warning: "Use String.fromCharCode() instead." },
    AscWarning { code: kUnsupportedMeth_ord, p_warning: "Use String.charCodeAt() instead." },
    AscWarning { code: kUnsupportedMeth_mbord, p_warning: "Use String.charCodeAt() instead." },
    AscWarning { code: kUnsupportedMeth_substring, p_warning: "Use the String.substr method instead." },
    AscWarning { code: kUnsupportedMeth_mbsubstring, p_warning: "Use the String.substr method instead." },
    AscWarning { code: kUnsupportedMeth_length, p_warning: "Use the length property of the argument instead." },
    AscWarning { code: kUnsupportedMeth_mblength, p_warning: "Use the length property of the argument instead." },
    AscWarning { code: kUnsupportedMeth_ASNative, p_warning: "For more information, see changes to ASnative." },
    AscWarning { code: kUnsupportedMeth_addProperty, p_warning: "Set properties directly on the instance using dot (.) notation instead." },
    AscWarning { code: kUnsupportedMeth_getProperty, p_warning: "Access properties directly using dot (.) notation instead." },
    AscWarning { code: kUnsupportedMeth_setProperty, p_warning: "Set properties directly on the instance using dot (.) notation instead." },
    AscWarning { code: kUnsupportedMeth_asfunction, p_warning: "For more information, see TextEvent.LINK and addEventListener()." },
    AscWarning { code: kUnsupportedMeth_clearInterval, p_warning: "This method has moved to the flash.utils package." },
    AscWarning { code: kUnsupportedMeth_duplicateMovieClip, p_warning: "Replaced by new MovieClip class constructor function." },
    AscWarning { code: kUnsupportedMeth_fscommand, p_warning: "Moved to flash.system package. Also, please see flash.external.ExternalInterface class for Javascript/ActionScript communication." },
    AscWarning { code: kUnsupportedMeth_getURL, p_warning: "For equivalent functionality, see flash.net.URLLoader.  The flash.net package also contains package-level functions navigateToURL() and sendToURL()." },
    AscWarning { code: kUnsupportedMeth_gotoAndPlay, p_warning: "For more information, see MovieClip.gotoAndPlay()." },
    AscWarning { code: kUnsupportedMeth_gotoAndStop, p_warning: "For more information, see MovieClip.gotoAndStop()." },
    AscWarning { code: kUnsupportedMeth_play, p_warning: "For more information, see MovieClip.play()." },
    AscWarning { code: kUnsupportedMeth_print, p_warning: "For more information, see PrintJob.start()." },
    AscWarning { code: kUnsupportedMeth_printAsBitmap, p_warning: "For more information, see PrintJob." },
    AscWarning { code: kUnsupportedMeth_printAsBitmapNum, p_warning: "For more information, see PrintJob." },
    AscWarning { code: kUnsupportedMeth_printNum, p_warning: "For more information, see PrintJob." },
    AscWarning { code: kUnsupportedMeth_removeMovieClip, p_warning: "Use Container.removeChild(childName). For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_setInterval, p_warning: "Moved to the flash.utils package.  Consider using the Timer class instead." },
    AscWarning { code: kUnsupportedMeth_nextFrame, p_warning: "For more information, see MovieClip.nextFrame()." },
    AscWarning { code: kUnsupportedMeth_startDrag, p_warning: "For more information, see MovieClip.startDrag()." },
    AscWarning { code: kUnsupportedMeth_stop, p_warning: "For more information, see MovieClip.stop()." },
    AscWarning { code: kUnsupportedMeth_stopAllSounds, p_warning: "For more information, see Sound.stopAllSounds()." },
    AscWarning { code: kUnsupportedMeth_stopDrag, p_warning: "For more information, see MovieClip.stopDrag()." },
    AscWarning { code: kUnsupportedMeth_tellTarget, p_warning: "Use the dot (.) operator or the with statement instead." },
    AscWarning { code: kUnsupportedMeth_toggleHighQuality, p_warning: "For more information, see DisplayObject.stage and Stage.quality." },
    AscWarning { code: kUnsupportedMeth_unloadMovie, p_warning: "Use DisplayObjectContainer.removeChild(childName) instead. For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_unloadMovieNum, p_warning: "Use DisplayObjectContainer.removeChild(childName) instead. For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_updateAfterEvent, p_warning: "This function is no longer a global function, but is still available as a method of the TimerEvent, MouseEvent, and KeyboardEvent classes." },
    AscWarning { code: kUnsupportedMeth_Video_attachVideo, p_warning: "For more information, see Video.attachNetStream, Video.attachCamera." },
    AscWarning { code: kUnsupportedMeth_TextField_StyleSheet_load, p_warning: "Use the URLLoader class to perform loading and pass the result to StyleSheet.parseCSS()" },
    AscWarning { code: kUnsupportedMeth_Object_registerClass, p_warning: "In ActionScript 3.0 all classes are registered by default.  If you are using AMF, see flash.utils.registerClassAlias() for more information." },
    AscWarning { code: kUnsupportedMeth_Object_unwatch, p_warning: "Use accessor properties (get/set functions) or the flash.utils.Proxy class for similar functionality." },
    AscWarning { code: kUnsupportedMeth_Object_watch, p_warning: "Use accessor properties (get/set functions) or the flash.utils.Proxy class for similar functionality." },
    AscWarning { code: kUnsupportedMeth_loadMovie, p_warning: "For more information, see MovieClip.loadMovie()." },
    AscWarning { code: kUnsupportedMeth_loadMovieNum, p_warning: "For more information, see MovieClip.loadMovieNum()." },
    AscWarning { code: kUnsupportedMeth_loadVariables, p_warning: "For more information, see Loader.load()." },
    AscWarning { code: kUnsupportedMeth_loadVariablesNum, p_warning: "For more information, see Loader.load()." },
    AscWarning { code: kUnsupportedMeth_Stage_addListener, p_warning: "For more information, see addEventListener ( eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_Stage_removeListener, p_warning: "For more information, see removeEventListener ( eventName, listener, useCapture)." },
    AscWarning { code: kUnsupportedMeth_TextField_addListener, p_warning: "For more information, see addEventListener ( eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_TextField_removeListener, p_warning: "For more information, see removeEventListener ( eventName, listener, useCapture)." },
    AscWarning { code: kUnsupportedMeth_Mouse_addListener, p_warning: "For more information, see addEventListener ( eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_Mouse_removeListener, p_warning: "For more information, see removeEventListener ( eventName, listener, useCapture)." },
    AscWarning { code: kUnsupportedMeth_Keyboard_addListener, p_warning: "For more information, see addEventListener ( eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_Keyboard_removeListener, p_warning: "For more information, see removeEventListener ( eventName, listener, useCapture)." },
    AscWarning { code: kUnsupportedMeth_Sound_attachSound, p_warning: "Use SWF class to create sounds from library" },
    AscWarning { code: kWarning_Event_onStatus, p_warning: "The onStatus event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'status', callback_handler)." },
    AscWarning { code: kWarning_Event_onID3, p_warning: "The onID3 event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'id3', callback_handler)." },
    AscWarning { code: kWarning_Event_onLoad, p_warning: "The onLoad event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'load', callback_handler)." },
    AscWarning { code: kWarning_Event_onSoundComplete, p_warning: "The onSoundComplete event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'soundComplete', callback_handler)." },
    AscWarning { code: kWarning_Event_onSetFocus, p_warning: "The onSetFocus event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'focusIn', callback_handler)." },
    AscWarning { code: kWarning_Event_onResize, p_warning: "The onResize event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'resize', callback_handler)." },
    AscWarning { code: kWarning_Event_onChanged, p_warning: "The onChanged event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'change', callback_handler)." },
    AscWarning { code: kWarning_Event_onKillFocus, p_warning: "The onKillFocus event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'focusOut', callback_handler)." },
    AscWarning { code: kWarning_Event_onScroller, p_warning: "The onScroller event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'scroll', callback_handler)." },
    AscWarning { code: kWarning_Event_onMouseDown, p_warning: "The onMouseDown event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseDown', callback_handler)." },
    AscWarning { code: kWarning_Event_onMouseUp, p_warning: "The onMouseUp event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseUp', callback_handler)." },
    AscWarning { code: kWarning_Event_onMouseMove, p_warning: "The onMouseMove event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseMove', callback_handler)." },
    AscWarning { code: kWarning_Event_onMouseWheel, p_warning: "The onMouseWheel event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseWheel', callback_handler)." },
    AscWarning { code: kWarning_Event_onKeyDown, p_warning: "The onKeyDown event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'keyDown', callback_handler)." },
    AscWarning { code: kWarning_Event_onKeyUp, p_warning: "The onKeyUp event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'keyUp', callback_handler)." },
    AscWarning { code: kWarning_Event_onData, p_warning: "The onData event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'data', callback_handler)." },
    AscWarning { code: kWarning_Event_onHTTPStatus, p_warning: "The onHTTPStatus event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'httpStatus', callback_handler)." },
    AscWarning { code: kWarning_Event_onDragOut, p_warning: "The onDragOut event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseOut', callback_handler)." },
    AscWarning { code: kWarning_Event_onDragOver, p_warning: "The onDragOver event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseOver', callback_handler)." },
    AscWarning { code: kWarning_Event_onPress, p_warning: "The onPress event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseDown', callback_handler)." },
    AscWarning { code: kWarning_Event_onRelease, p_warning: "The onRelease event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'click', callback_handler)." },
    AscWarning { code: kWarning_Event_onReleaseOutside, p_warning: "The onReleaseOutside event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseUp', callback_handler)." },
    AscWarning { code: kWarning_Event_onRollOut, p_warning: "The onRollOut event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseOut', callback_handler)." },
    AscWarning { code: kWarning_Event_onRollOver, p_warning: "The onRollOver event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'mouseOver', callback_handler)." },
    AscWarning { code: kWarning_Event_onActivity, p_warning: "The onActivity event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'activity', callback_handler)." },
    AscWarning { code: kWarning_Event_onSelect, p_warning: "The onSelect event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'menuSelect', callback_handler)." },
    AscWarning { code: kWarning_Event_onEnterFrame, p_warning: "The onEnterFrame is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'enterFrame', callback_handler)." },
    AscWarning { code: kWarning_Event_onUnload, p_warning: "The onUnload event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'unload', callback_handler)." },
    AscWarning { code: kWarning_Event_onLoadComplete, p_warning: "The onLoadComplete is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'load', callback_handler)." },
    AscWarning { code: kWarning_Event_onLoadError, p_warning: "The onLoadError event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'error', callback_handler)." },
    AscWarning { code: kWarning_Event_onLoadInit, p_warning: "The onLoadInit event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'init', callback_handler)." },
    AscWarning { code: kWarning_Event_onLoadProgress, p_warning: "The onLoadProgress event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'progress', callback_handler)." },
    AscWarning { code: kWarning_Event_onLoadStart, p_warning: "The onLoadStart is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'start', callback_handler)." },
    AscWarning { code: kWarning_Event_onClose, p_warning: "The onClose event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'close', callback_handler)." },
    AscWarning { code: kWarning_Event_onConnect, p_warning: "The onConnect event handler is not triggered automatically by Flash player in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'connect', callback_handler)." },
    AscWarning { code: kWarning_Event_onXML, p_warning: "onXML is not triggered automatically by Flash Player in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( 'xml', callback_handler)." },
    AscWarning { code: kUnsupportedMeth_hasChildNodes, p_warning: "Use the property hasChildNodes instead." },
    AscWarning { code: kUnsupportedMeth_XMLEvent, p_warning: "The XMLEvent class is obsolete, and the xml event is no longer dispatched. Only the data event is dispatched during file loading." },
    AscWarning { code: kUnsupportedMeth_XMLDoc, p_warning: "The XMLDoc class has been renamed XMLDocument." },
    AscWarning { code: kUnsupportedMeth_Accessibility_isActive, p_warning: "Use the Accessibility.active property instead." },
    AscWarning { code: kUnsupportedProp_ActivityEvent_ACTIVITY_TYPE, p_warning: "Use ActivityEvent.ACTIVITY constant instead." },
    AscWarning { code: kUnsupportedMeth_SimpleButton_getDepth, p_warning: "Use DisplayObjectContainer.parent.getChildIndex instead. See help for the DisplayObjectContainer class, which is extended by MovieClip." },
    AscWarning { code: kUnsupportedMeth_SimpleButton_swapDepths, p_warning: "Use DisplayObjectContainer.parent.setChildIndex instead. See help for the DisplayObjectContainer class, which is extended by MovieClip." },
    AscWarning { code: kUnsupportedMeth_SimpleButton_getInstanceAtDepth, p_warning: "Use DisplayObjectContainer.getChildAt instead. See help for the DisplayObjectContainer class, which is extended by MovieClip." },
    AscWarning { code: kUnsupportedMeth_SimpleButton_getNextHighestDepth, p_warning: "Use DisplayObjectContainer.numChildren instead. DisplayObjectContainer.addChild always adds the new child to index DisplayObjectContainer.numChildren." },
    AscWarning { code: kUnsupportedMeth_ByteArray_available, p_warning: "Use the ByteArray.bytesAvailable property instead." },
    AscWarning { code: kUnsupportedMeth_ByteArray_getFilePointer, p_warning: "Use the ByteArray.position property instead." },
    AscWarning { code: kUnsupportedMeth_ByteArray_seek, p_warning: "Use the ByteArray.position property instead." },
    AscWarning { code: kUnsupportedMeth_Camera_get, p_warning: "Use the Camera.getCamera() method instead." },
    AscWarning { code: kUnsupportedProp_Camera_currentFps, p_warning: "Use the Camera.currentFPS property instead." },
    AscWarning { code: kUnsupportedMeth_Camera_setKeyFrameInterval, p_warning: "Use the Camera.keyFrameInterval property instead." },
    AscWarning { code: kUnsupportedMeth_Camera_setLoopback, p_warning: "Use the Camera.loopback property instead." },
    AscWarning { code: kUnsupportedMeth_ColorTransform_getRGB, p_warning: "Use the ColorTransform.color property instead." },
    AscWarning { code: kUnsupportedMeth_ColorTransform_setRGB, p_warning: "Use the ColorTransform.color property instead." },
    AscWarning { code: kUnsupportedMeth_DisplayObjectContainer_getTextSnapshot, p_warning: "Use the Container.textSnapshot property instead." },
    AscWarning { code: kUnsupportedMeth_ContextMenu_copy, p_warning: "Use the ContextMenu.clone() method instead." },
    AscWarning { code: kUnsupportedProp_ContextMenu_forward_back, p_warning: "Use the ContextMenu.forwardAndBack property instead." },
    AscWarning { code: kUnsupportedMeth_ContextMenuItem_copy, p_warning: "Use the ContextMenuItem.clone() method instead." },
    AscWarning { code: kUnsupportedMeth_CustomActions_listActions, p_warning: "Use the CustomActions.actionsList property instead." },
    AscWarning { code: kUnsupportedProp_DataEvent_DATA_TYPE, p_warning: "Use the DataEvent.DATA constant instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject_xscale, p_warning: "Use the DisplayObject.scaleX property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__xscale, p_warning: "Use the DisplayObject.scaleX property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject_yscale, p_warning: "Use the DisplayObject.scaleY property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__yscale, p_warning: "Use the DisplayObject.scaleY property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject_xmouse, p_warning: "Use the DisplayObject.mouseX property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__xmouse, p_warning: "Use the DisplayObject.mouseX property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject_ymouse, p_warning: "Use the DisplayObject.mouseY property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__ymouse, p_warning: "Use the DisplayObject.mouseY property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject_setRootClass, p_warning: "This feature is no longer supported." },
    AscWarning { code: kUnsupportedProp_DisplayObject_mapSymbolToClass, p_warning: "This feature is no longer supported." },
    AscWarning { code: kUnsupportedProp_DisplayObject__name, p_warning: "Use the DisplayObject.name property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__parent, p_warning: "Use the DisplayObject.parent property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__mask, p_warning: "Use the DisplayObject.mask property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__visible, p_warning: "Use the DisplayObject.visible property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__x, p_warning: "Use the DisplayObject.x property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__y, p_warning: "Use the DisplayObject.y property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__rotation, p_warning: "Use the DisplayObject.rotation property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__alpha, p_warning: "Use the DisplayObject.alpha property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__width, p_warning: "Use the DisplayObject.width property instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject__height, p_warning: "Use the DisplayObject.height property instead." },
    AscWarning { code: kUnsupportedMeth_ExternalInterface_available, p_warning: "Use the ExternalInterface.available property instead." },
    AscWarning { code: kUnsupportedProp_ErrorEvent_ERROR_TYPE, p_warning: "Use the ErrorEvent.ERROR constant instead." },
    AscWarning { code: kUnsupportedMeth_Event_isDefaultPrevented, p_warning: "Use the Event.isDefaultPrevented property instead." },
    AscWarning { code: kUnsupportedProp_Event_ACTIVATE_TYPE, p_warning: "Use the Event.ACTIVATE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_ADDED_TYPE, p_warning: "Use the Event.ADDED constant instead." },
    AscWarning { code: kUnsupportedProp_Event_CANCEL_TYPE, p_warning: "Use the Event.CANCEL constant instead." },
    AscWarning { code: kUnsupportedProp_Event_CHANGE_TYPE, p_warning: "Use the Event.CHANGE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_CLOSE_TYPE, p_warning: "Use the Event.CLOSE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_COMPLETE_TYPE, p_warning: "Use the Event.COMPLETE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_CONNECT_TYPE, p_warning: "Use the Event.CONNECT constant instead." },
    AscWarning { code: kUnsupportedProp_Event_DEACTIVATE_TYPE, p_warning: "Use the Event.DEACTIVATE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_ENTER_FRAME_TYPE, p_warning: "Use the Event.ENTER_FRAME constant instead." },
    AscWarning { code: kUnsupportedProp_Event_ID3_TYPE, p_warning: "Use the Event.ID3 constant instead." },
    AscWarning { code: kUnsupportedProp_Event_INIT_TYPE, p_warning: "Use the Event.INIT constant instead." },
    AscWarning { code: kUnsupportedProp_Event_MOUSE_LEAVE_TYPE, p_warning: "Use the Event.MOUSE_LEAVE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_OPEN_TYPE, p_warning: "Use the Event.OPEN constant instead." },
    AscWarning { code: kUnsupportedProp_Event_REMOVED_TYPE, p_warning: "Use the Event.REMOVED constant instead." },
    AscWarning { code: kUnsupportedProp_Event_RENDER_TYPE, p_warning: "Use the Event.RENDER constant instead." },
    AscWarning { code: kUnsupportedProp_Event_RESIZE_TYPE, p_warning: "Use the Event.RESIZE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_SCROLL_TYPE, p_warning: "Use the Event.SCROLL constant instead." },
    AscWarning { code: kUnsupportedProp_Event_SELECT_TYPE, p_warning: "Use the Event.SELECT constant instead." },
    AscWarning { code: kUnsupportedProp_Event_SOUND_COMPLETE_TYPE, p_warning: "Use the Event.SOUND_COMPLETE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_TAB_CHILDREN_CHANGE_TYPE, p_warning: "Use the Event.TAB_CHILDREN_CHANGE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_TAB_ENABLED_CHANGE_TYPE, p_warning: "Use the Event.TAB_ENABLED_CHANGE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_TAB_INDEX_CHANGE_TYPE, p_warning: "Use the Event.TAB_INDEX_CHANGE constant instead." },
    AscWarning { code: kUnsupportedProp_Event_UNLOAD_TYPE, p_warning: "Use the Event.UNLOAD constant instead." },
    AscWarning { code: kUnsupportedProp_FocusEvent_FOCUS_IN_TYPE, p_warning: "Use the FocusEvent.FOCUS_IN constant instead." },
    AscWarning { code: kUnsupportedProp_FocusEvent_FOCUS_OUT_TYPE, p_warning: "Use the FocusEvent.FOCUS_OUT constant instead." },
    AscWarning { code: kUnsupportedProp_FocusEvent_KEY_FOCUS_CHANGE_TYPE, p_warning: "Use the FocusEvent.KEY_FOCUS_CHANGE constant instead." },
    AscWarning { code: kUnsupportedProp_FocusEvent_MOUSE_FOCUS_CHANGE_TYPE, p_warning: "Use the FocusEvent.MOUSE_FOCUS_CHANGE constant instead." },
    AscWarning { code: kUnsupportedMeth_Graphics_beginImageFill, p_warning: "Use the Graphics.beginBitmapFill() method instead." },
    AscWarning { code: kUnsupportedProp_BitmapFilter_XYZ_QUALITY, p_warning: "Use the BitmapFilter.quality property instead." },
    AscWarning { code: kUnsupportedMeth_Keyboard_getAscii, p_warning: "Use KeyboardEvent.charCode instead." },
    AscWarning { code: kUnsupportedMeth_Keyboard_getCode, p_warning: "Use KeyboardEvent.keyCode instead." },
    AscWarning { code: kUnsupportedMeth_Keyboard_isDown, p_warning: "For more information, see the KeyboardEvent class." },
    AscWarning { code: kUnsupportedMeth_Keyboard_isToggled, p_warning: "For more information, see KeyboardEvent.ctrlKey, KeyboardEvent.altKey, and KeyboardEvent.shiftKey." },
    AscWarning { code: kUnsupportedProp_Keyboard_PGDN, p_warning: "Use the PAGE_DOWN constant instead." },
    AscWarning { code: kUnsupportedProp_Keyboard_PGUP, p_warning: "Use the PAGE_UP constant instead." },
    AscWarning { code: kUnsupportedProp_Keyboard_DELETEKEY, p_warning: "Use the DELETE constant instead." },
    AscWarning { code: kUnsupportedProp_Keyboard_CAPSLOCK, p_warning: "Use the CAPS_LOCK constant instead." },
    AscWarning { code: kUnsupportedProp_Keyboard_NUMPAD, p_warning: "Use one of the NUMPAD_0 -> NUMPAD_9 constants instead." },
    AscWarning { code: kUnsupportedProp_InteractiveObject__focusrect, p_warning: "Use the InteractiveObject.focusRect property instead." },
    AscWarning { code: kUnsupportedProp_InteractiveObject_menu, p_warning: "Use the InteractiveObject.contextMenu property instead." },
    AscWarning { code: kUnsupportedProp_KeyboardEvent_ascii, p_warning: "Use the KeyboardEvent.charCode property instead." },
    AscWarning { code: kUnsupportedProp_KeyboardEvent_code, p_warning: "Use the KeyboardEvent.keyCode property instead." },
    AscWarning { code: kUnsupportedProp_Loader_loaderInfo, p_warning: "Use the Loader.contentLoaderInfo property instead." },
    AscWarning { code: kUnsupportedProp_Loader_preload, p_warning: "This feature is no longer supported." },
    AscWarning { code: kUnsupportedProp_Loader_loadCached, p_warning: "This feature is no longer supported." },
    AscWarning { code: kUnsupportedProp_LoaderInfo_loaderUrl, p_warning: "Use the LoaderInfo.loaderURL property instead." },
    AscWarning { code: kUnsupportedMeth_LocalConnection_domain, p_warning: "Use the LocalConnection.domain property instead." },
    AscWarning { code: kUnsupportedProp_ContextMenuEvent_menuOwner, p_warning: "Use the MenuEvent.contextMenuOwner property instead." },
    AscWarning { code: kUnsupportedMeth_Microphone_get, p_warning: "Use the Microphone.getMicrophone() method instead." },
    AscWarning { code: kUnsupportedMeth_MovieClip_attachMovie, p_warning: "If the MovieClip subclass name is A use var mc= new A(); addChild(mc). For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_createEmptyMovieClip, p_warning: "Use var mc= new MovieClip(); addChild(mc). For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_createTextField, p_warning: "Use var tf= new TextField(); addChild(mc). For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_removeMovieClip, p_warning: "Use Container.removeChild(childName). For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_loadMovie, p_warning: "Use var l = new Loader(); addChild(l); l.load(new URLRequest(\"your url\"));. For more information, see the Loader and DisplayObjectContainer classes." },
    AscWarning { code: kUnsupportedMeth_MovieClip_unloadMovie, p_warning: "Use DisplayObjectContainer.removeChild(childName) instead. For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_unloadMovieNum, p_warning: "Use DisplayObjectContainer.removeChild(childName) instead. For more information, see the DisplayObjectContainer class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getDepth, p_warning: "Use DisplayObjectContainer.parent.getChildIndex instead. For more information, see the DisplayObjectContainer class, is extended by MovieClip." },
    AscWarning { code: kUnsupportedMeth_MovieClip_swapDepths, p_warning: "Use DisplayObjectContainer.parent.setChildIndex instead. For more information, see the DisplayObjectContainer class, which is extended by MovieClip." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getInstanceAtDepth, p_warning: "Use DisplayObjectContainer.getChildAt instead. For more information, see the DisplayObjectContainer class, which is extended by MovieClip." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getNextHighestDepth, p_warning: "Use DisplayObjectContainer.numChildren instead. DisplayObjectContainer.addChild always adds the new child to index DisplayObjectContainer.numChildren." },
    AscWarning { code: kUnsupportedMeth_MovieClip_attachAudio, p_warning: "For more information, see DisplayObject.addChild." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getBytesLoaded, p_warning: "For more information, see LoaderInfo.bytesLoaded and the Loader class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getBytesTotal, p_warning: "For more information, see LoaderInfo.bytesTotal and the Loader class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getURL, p_warning: "For equivalent functionality, see flash.net.URLLoader.  The flash.net package also contains package-level functions navigateToURL() and sendToURL()." },
    AscWarning { code: kUnsupportedProp_MovieClip_url, p_warning: "For more information, see LoaderInfo.url and the Loader class." },
    AscWarning { code: kUnsupportedProp_MovieClip__url, p_warning: "For more information, see LoaderInfo.url and the Loader class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_setMask, p_warning: "Use the MovieClip.mask property instead." },
    AscWarning { code: kUnsupportedMeth_MovieClip_getSWFVersion, p_warning: "For more information, see LoaderInfo.swfVersion and the Loader class." },
    AscWarning { code: kUnsupportedProp_MovieClip__currentframe, p_warning: "Use the MovieClip.currentFrame property instead." },
    AscWarning { code: kUnsupportedProp_MovieClip__framesloaded, p_warning: "Use the MovieClip.framesLoaded property instead." },
    AscWarning { code: kUnsupportedProp_MovieClip__totalframes, p_warning: "Use the MovieClip.totalFrames property instead." },
    AscWarning { code: kUnsupportedProp_MovieClip_lockroot, p_warning: "For more information, see displayObjectInstance.root." },
    AscWarning { code: kUnsupportedProp_MovieClip__lockroot, p_warning: "For more information, see displayObjectInstance.root." },
    AscWarning { code: kUnsupportedProp_MovieClip__soundbuftime, p_warning: "Use the static propery flash.media.SoundMixer.bufferTime instead." },
    AscWarning { code: kUnsupportedMeth_MovieClip_clear, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_beginFill, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_beginGradientFill, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_lineGradientStyle, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_beginImageFill, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_lineStyle, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_drawRect, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_drawRoundRect, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_drawRoundRectComplex, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_drawCircle, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_moveTo, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_lineTo, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_curveTo, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_endFill, p_warning: "For more information, see the Graphics class." },
    AscWarning { code: kUnsupportedMeth_NetStream_setBufferTime, p_warning: "Use the NetStream.bufferTime property instead." },
    AscWarning { code: kUnsupportedProp_NetStream_currentFps, p_warning: "Use the NetStream.currentFPS property instead." },
    AscWarning { code: kUnsupportedProp_NetStream_videocodec, p_warning: "Use the NetStream.videoCodec property instead." },
    AscWarning { code: kUnsupportedProp_NetStream_audiocodec, p_warning: "Use the NetStream.audioCodec property instead." },
    AscWarning { code: kUnsupportedMeth_ProductManager_isIntalled, p_warning: "Use the ProductManager.isInstalled property instead." },
    AscWarning { code: kUnsupportedMeth_ProductManager_installedVersion, p_warning: "Use the ProductManager.installedVersion property instead." },
    AscWarning { code: kUnsupportedMeth_ProductManager_isRunning, p_warning: "Use the ProductManager.isRunning property instead." },
    AscWarning { code: kUnsupportedMeth_Point_addTo, p_warning: "Use the Point.add() method instead." },
    AscWarning { code: kUnsupportedProp_Proxy_delDescendants, p_warning: "Use the Proxy.deleteDescendants property instead." },
    AscWarning { code: kUnsupportedMeth_Profiler_heapdump, p_warning: "Use the heapDump() method instead." },
    AscWarning { code: kUnsupportedProp_ProgressEvent_current, p_warning: "Use the ProgressEvent.bytesLoaded property instead." },
    AscWarning { code: kUnsupportedProp_ProgressEvent_total, p_warning: "Use the ProgressEvent.bytesTotal property instead." },
    AscWarning { code: kUnsupportedMeth_Rectangle_isEmpty, p_warning: "Use the Rectangle.isEmpty property instead." },
    AscWarning { code: kUnsupportedMeth_SoundTransform_setPan, p_warning: "Use the SoundTransform.pan property instead." },
    AscWarning { code: kUnsupportedMeth_Socket_available, p_warning: "Use the Sockect.bytesAvailable property instead." },
    AscWarning { code: kUnsupportedMeth_SharedObject_getSize, p_warning: "Use the SharedObject.size property instead." },
    AscWarning { code: kUnsupportedMeth_SharedObject_setFps, p_warning: "Use the SharedObject.fps property instead." },
    AscWarning { code: kUnsupportedMeth_Sprite_getSWF, p_warning: "This is no longer supported." },
    AscWarning { code: kUnsupportedMeth_Sprite_constructChild, p_warning: "Use the Sprite.constructChildren() method instead." },
    AscWarning { code: kUnsupportedProp_Sprite__droptarget, p_warning: "Use the Sprite.dropTarget property instead." },
    AscWarning { code: kUnsupportedMeth_Stage_getFocus, p_warning: "Use the Stage.focus property instead." },
    AscWarning { code: kUnsupportedMeth_Stage_setFocus, p_warning: "Use the Stage.focus property instead." },
    AscWarning { code: kUnsupportedProp_Stage_showMenu, p_warning: "Use the Stage.showDefaultContextMenu property instead." },
    AscWarning { code: kUnsupportedMeth_StyleSheet_getStyleNames, p_warning: "Use the StyleSheet.styleNames property instead." },
    AscWarning { code: kUnsupportedMeth_StyleSheet_onData, p_warning: "Use an instance of URLLoader to load the StyleSheet data, and then pass the loaders data to the StyleSheet.parseCSS method. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_StyleSheet_load, p_warning: "Use an instance of URLLoader to load the StyleSheet data, and then pass the loaders data to the StyleSheet.parseCSS method. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedProp_StyleSheet_loaded, p_warning: "Use an instance of URLLoader to load the StyleSheet data, and then pass the loaders data to the StyleSheet.parseCSS method. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_StyleSheet_getBytesLoaded, p_warning: "Use an instance of URLLoader to load the StyleSheet data, and then pass the loaders data to the StyleSheet.parseCSS method. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_StyleSheet_getBytesTotal, p_warning: "Use an instance of URLLoader to load the StyleSheet data, and then pass the loaders data to the StyleSheet.parseCSS method. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_IME_getEnabled, p_warning: "Use the IME.enabled property instead." },
    AscWarning { code: kUnsupportedMeth_IME_setEnabled, p_warning: "Use the IME.enabled property instead." },
    AscWarning { code: kUnsupportedMeth_IME_getInstance, p_warning: "Use the IME.instance property instead." },
    AscWarning { code: kUnsupportedMeth_IME_getConversionMode, p_warning: "Use the IME.conversionMode property instead." },
    AscWarning { code: kUnsupportedMeth_IME_setConversionMode, p_warning: "Use the IME.conversionMode property instead." },
    AscWarning { code: kUnsupportedProp_System_getAvmplusVersion, p_warning: "Use the System.vmVersion property instead." },
    AscWarning { code: kUnsupportedProp_SWFLoaderInfo_SWFVersion, p_warning: "Use the SWFLoaderInfo.swfVersion property instead." },
    AscWarning { code: kUnsupportedProp_SWFLoaderInfo_ASVersion, p_warning: "Use the SWFLoaderInfo.actionScriptVersion property instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getNewTextFormat, p_warning: "Use the TextField.defaultTextFormat property instead." },
    AscWarning { code: kUnsupportedMeth_TextField_setNewTextFormat, p_warning: "Use the TextField.defaultTextFormat property instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getDepth, p_warning: "Use DisplayObjectContainer.parent.getChildIndex instead. For more information, see the DisplayObjectContainer class, which MovieClip extends." },
    AscWarning { code: kUnsupportedMeth_TextField_swapDepths, p_warning: "Use DisplayObjectContainer.parent.setChildIndex instead. For more information, see the DisplayObjectContainer class, which MovieClip extends." },
    AscWarning { code: kUnsupportedMeth_TextField_getInstanceAtDepth, p_warning: "Use DisplayObjectContainer.getChildAt instead. For more information, see the DisplayObjectContainer class, which MovieClip extends." },
    AscWarning { code: kUnsupportedMeth_TextField_getNextHighestDepth, p_warning: "Use DisplayObjectContainer.numChildren instead.  DisplayObjectContainer.addChild always adds the new child to index DisplayObjectContainer.numChildren." },
    AscWarning { code: kUnsupportedMeth_TextField_replaceSel, p_warning: "Use the TextField.replaceSelectedText() method instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getLineIndexOfCharacter, p_warning: "Use the TextField.getLineIndexOfChar() method instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getSelectionBeginIndex, p_warning: "Use the TextField.selectionBeginIndex property instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getSelectionEndIndex, p_warning: "Use the TextField.selectionEndIndex property instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getCaretIndex, p_warning: "Use the TextField.caretIndex property instead." },
    AscWarning { code: kUnsupportedMeth_TextField_getFontList, p_warning: "Use the Font.enumerateFonts() method instead." },
    AscWarning { code: kUnsupportedProp_TextField_maxscroll, p_warning: "Use the TextField.maxScrollV property instead." },
    AscWarning { code: kUnsupportedProp_TextField_hscroll, p_warning: "Use the TextField.scrollH property instead." },
    AscWarning { code: kUnsupportedProp_TextField_maxhscroll, p_warning: "Use the TextField.maxScrollH property instead." },
    AscWarning { code: kUnsupportedProp_TextField_newTextFormat, p_warning: "Use the TextField.defaultTextFormat property instead." },
    AscWarning { code: kUnsupportedMeth_TextFormat_getTextExtent, p_warning: "Create a temporary TextField and use TextField.getLineMetrics instead." },
    AscWarning { code: kUnsupportedMeth_TextSnapshot_getCount, p_warning: "Use the TextSnapshot.charCount property instead." },
    AscWarning { code: kUnsupportedProp_URLLoader_navigate, p_warning: "Use the navigateToURL() method in the flash.net package instead." },
    AscWarning { code: kUnsupportedMeth_URLLoader_send, p_warning: "Use the sendToURL() method in the flash.net package instead." },
    AscWarning { code: kUnsupportedProp_URLLoader_binary, p_warning: "Use the URLLoader.dataFormat property instead." },
    AscWarning { code: kUnsupportedMeth_URLStream_available, p_warning: "Use the URLStream.bytesAvailable property instead." },
    AscWarning { code: kUnsupportedProp_URLRequest_digest, p_warning: "This property is no longer supported." },
    AscWarning { code: kUnsupportedProp_URLRequest_importToSandbox, p_warning: "Use the URLRequest.applicationDomain property instead." },
    AscWarning { code: kUnsupportedMeth_URLRequest_addRequestHeader, p_warning: "To add request headers, set the URLRequest.requestHeaders property to an array of URLRequestHeader objects." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_load, p_warning: "Use an instance of URLLoader to load the XML file, then pass the URLLoaders data to the XMLDocuments constructor. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_send, p_warning: "Use the sendToURL() method in the flash.net package instead." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_sendAndLoad, p_warning: "Set a URLRequest object postData property and use it with a URLLoader object to load the XML file.  Pass the URLLoaders data to the XMLDocuments constructor. For more information, see the URLLoader, URLRequest and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_onData, p_warning: "Use an instance of URLLoader to load the XML file, then pass the URLLoaders data to the XMLDocuments constructor. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_addRequestHeader, p_warning: "To add request headers, set the URLRequest.requestHeaders property to an array of URLRequestHeader objects." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_getBytesLoaded, p_warning: "For more information, see URLLoader.bytesLoaded and the URLLoader class." },
    AscWarning { code: kUnsupportedMeth_XMLDocument_getBytesTotal, p_warning: "For more information, see URLLoader.bytesTotal and the URLLoader class." },
    AscWarning { code: kUnsupportedProp_XMLDocument_loaded, p_warning: "Use an instance of URLLoader to load the XML file, then pass the loaders data to the StyleSheet.parseCSS method. For more information, see the URLLoader and EventDispatcher classes." },
    AscWarning { code: kUnsupportedProp_XMLDocument_contentType, p_warning: "Use the URLRequest.contentType property instead." },
    AscWarning { code: kUnsupportedProp_XMLDocument_status, p_warning: "Check for the possible exceptions thrown by the XMLDocument constructor or the XMLDocument.parseXML method instead. For more information, see XMLDocument." },
    AscWarning { code: kUnsupportedProp_Button, p_warning: "The Button class has been renamed SimpleButton." },
    AscWarning { code: kUnsupportedProp_Container, p_warning: "The Container class has been renamed DisplayObjectContainer." },
    AscWarning { code: kUnsupportedProp_Image, p_warning: "The Image class has been renamed BitmapData." },
    AscWarning { code: kUnsupportedProp_ImageFilter, p_warning: "The ImageFilter class has been renamed BitmapFilter." },
    AscWarning { code: kUnsupportedProp_ImageSprite, p_warning: "The ImageSprite class has been renamed Bitmap." },
    AscWarning { code: kUnsupportedProp_ImageLoaderInfo, p_warning: "The ImageLoaderInfo class has been renamed BitmapLoaderInfo." },
    AscWarning { code: kUnsupportedProp_ImeEvent, p_warning: "The ImeEvent class has been renamed IMEEvent." },
    AscWarning { code: kUnsupportedProp_Key, p_warning: "The Key class has been renamed Keyboard." },
    AscWarning { code: kUnsupportedProp_LineMetrics, p_warning: "The LineMetrics class has been renamed TextLineMetrics." },
    AscWarning { code: kUnsupportedProp_LoadVars, p_warning: "For more information, see the URLVariables class, the URLRequest.urlVariables and URLRequest.postData properties, and the URLLoader.dataFormat property." },
    AscWarning { code: kUnsupportedProp_MenuEvent, p_warning: "The MenuEvent class has been renamed ContextMenuEvent." },
    AscWarning { code: kUnsupportedProp_SystemCapabilities, p_warning: "The SystemCapabilities class has been renamed Capabilities." },
    AscWarning { code: kUnsupportedProp_TextExtents, p_warning: "Use the TextField.getLineMetrics property instead." },
    AscWarning { code: kUnsupportedMeth_Button, p_warning: "The Button class has been renamed SimpleButton." },
    AscWarning { code: kUnsupportedMeth_Container, p_warning: "The Container class has been renamed DisplayObjectContainer." },
    AscWarning { code: kUnsupportedMeth_Image, p_warning: "The Image class has been renamed BitmapData." },
    AscWarning { code: kUnsupportedMeth_ImageFilter, p_warning: "The ImageFilter class has been renamed BitmapFilter." },
    AscWarning { code: kUnsupportedMeth_ImageSprite, p_warning: "The ImageSprite class has been renamed Bitmap." },
    AscWarning { code: kUnsupportedMeth_ImageLoaderInfo, p_warning: "The ImageLoaderInfo class has been renamed BitmapLoaderInfo." },
    AscWarning { code: kUnsupportedMeth_ImeEvent, p_warning: "The ImeEvent class has been renamed IMEEvent." },
    AscWarning { code: kUnsupportedMeth_Key, p_warning: "The Key class has been renamed Keyboard." },
    AscWarning { code: kUnsupportedMeth_LineMetrics, p_warning: "The LineMetrics class has been renamed TextLineMetrics." },
    AscWarning { code: kUnsupportedMeth_LoadVars, p_warning: "For more information, see the URLVariables class, the URLRequest.urlVariables and URLRequest.postData properties, and the URLLoader.dataFormat property." },
    AscWarning { code: kUnsupportedMeth_MenuEvent, p_warning: "The MenuEvent class has been renamed ContextMenuEvent." },
    AscWarning { code: kUnsupportedMeth_SystemCapabilities, p_warning: "The SystemCapabilities class has been renamed Capabilities." },
    AscWarning { code: kUnsupportedMeth_TextExtents, p_warning: "Use the TextField.getLineMetrics property instead." },
    AscWarning { code: kUnsupportedProp_Object___resolve, p_warning: "For more information, see help for the Proxy class, which offers similar functionality." },
    AscWarning { code: kUnsupportedMeth_XMLUI_get, p_warning: "Use the XMLUI.getProperty method instead." },
    AscWarning { code: kUnsupportedMeth_XMLUI_set, p_warning: "Use the XMLUI.setProperty method instead." },
    AscWarning { code: kUnsupportedProp_DisplayObject_accProps, p_warning: "Use the DisplayObject.accessibilityProperties property instead." },
    AscWarning { code: kUnsupportedMeth_DisplayObject_setScalingGrid, p_warning: "Use the DisplayObject.scale9Grid property instead." },
    AscWarning { code: kUnsupportedMeth_Graphics_drawCircle, p_warning: "Use the Graphics.drawOval method instead." },
    AscWarning { code: kUnsupportedProp_NetConnection_isConnected, p_warning: "Use the NetConnection.connected property instead." },
    AscWarning { code: kUnsupportedProp_Socket_isConnected, p_warning: "Use the Socket.connected property instead." },
    AscWarning { code: kUnsupportedProp_URLStream_isConnected, p_warning: "Use the URLStream.connected property instead." },
    AscWarning { code: kUnsupportedProp_SyncEvent_list, p_warning: "Use the SyncEvent.changeList property instead." },
    AscWarning { code: kUnsupportedProp_TextField_scroll, p_warning: "Use the TextField.scrollV property instead." },
    AscWarning { code: kUnsupportedProp_TextField_bottomScroll, p_warning: "Use the TextField.bottomScrollV property instead." },
    AscWarning { code: kUnsupportedProp_BitmapData_RED_CHANNEL, p_warning: "Use the BitmapDataChannel.RED constant instead." },
    AscWarning { code: kUnsupportedProp_BitmapData_GREEN_CHANNEL, p_warning: "Use the BitmapDataChannel.GREEN constant instead." },
    AscWarning { code: kUnsupportedProp_BitmapData_BLUE_CHANNEL, p_warning: "Use the BitmapDataChannel.BLUE constant instead." },
    AscWarning { code: kUnsupportedProp_BitmapData_ALPHA_CHANNEL, p_warning: "Use the BitmapDataChannel.ALPHA constant instead." },
    AscWarning { code: kUnsupportedMeth_instanceof, p_warning: "Use the is operator instead." },
    AscWarning { code: kUnsupportedMeth_System_showSettings, p_warning: "Use the flash.system.Security.showSettings method instead." },
    AscWarning { code: kUnsupportedProp_System_useCodepage, p_warning: "Use the System.useCodePage property instead." },
    AscWarning { code: kUnsupportedProp_AsBroadcaster, p_warning: "Use the flash.events.EventDispatcher class instead." },
    AscWarning { code: kUnsupportedProp_SimpleButton_Soundbuftime, p_warning: "Use the static propery flash.media.SoundMixer.bufferTime instead." },
    AscWarning { code: kUnsupportedMeth_BitmapData_loadBitmap, p_warning: "Create a new instance of the bitmap library symbol class, i.e. new myBitmapName(), instead." },
    AscWarning { code: kUnsupportedMeth_MovieClip_loadVariables, p_warning: "For more information, see Loader.load()." },
    AscWarning { code: kUnsupportedProp_MovieClipLoader, p_warning: "The MovieClipLoader class has been replaced by the flash.display.Loader class." },
    AscWarning { code: kUnsupportedMeth_MovieClipLoader, p_warning: "The MovieClipLoader class has been replaced by the flash.display.Loader class." },
    AscWarning { code: kUnsupportedMeth_IME_addListener, p_warning: "For more information, see addEventListener(eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_IME_removeListener, p_warning: "For more information, see removeEventListener(eventName, listener, useCapture)." },
    AscWarning { code: kUnsupportedProp_IME_ALPHANUMERIC_FULL, p_warning: "Use the flash.system.IMEConversionMode.ALPHANUMERIC_FULL constant instead." },
    AscWarning { code: kUnsupportedProp_IME_ALPHANUMERIC_HALF, p_warning: "Use the flash.system.IMEConversionMode.ALPHANUMERIC_HALF constant instead." },
    AscWarning { code: kUnsupportedProp_IME_CHINESE, p_warning: "Use the flash.system.IMEConversionMode.CHINESE constant instead." },
    AscWarning { code: kUnsupportedProp_IME_JAPANESE_HIRAGANA, p_warning: "Use the flash.system.IMEConversionMode.JAPANESE_HIRAGANA constant instead." },
    AscWarning { code: kUnsupportedProp_IME_JAPANESE_KATAKANA_FULL, p_warning: "Use the flash.system.IMEConversionMode.JAPANESE_KATAKANA_FULL constant instead." },
    AscWarning { code: kUnsupportedProp_IME_JAPANESE_KATAKANA_HALF, p_warning: "Use the flash.system.IMEConversionMode.JAPANESE_KATAKANA_HALF constant instead." },
    AscWarning { code: kUnsupportedProp_IME_KOREAN, p_warning: "Use the flash.system.IMEConversionMode.KOREAN constant instead." },
    AscWarning { code: kUnsupportedProp_IME_UNKNOWN, p_warning: "Use the flash.system.IMEConversionMode.UNKNOWN constant instead." },
    AscWarning { code: kUnsupportedMeth_FileReferenceList_addListener, p_warning: "For more information, see addEventListener ( eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_FileReferenceList_removeListener, p_warning: "For more information, see removeEventListener ( eventName, listener, useCapture)." },
    AscWarning { code: kWarning_Event_onCancel, p_warning: "The onCancel event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( cancel, onCancel)." },
    AscWarning { code: kUnsupportedProp_Keyboard__listeners, p_warning: "There is no direct replacement. The willTrigger() method can be used to tell if any listeners have been registered." },
    AscWarning { code: kWarning_Event_onIMEComposition, p_warning: "The onIMEComposition event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ( imeComposition, handlerName)." },
    AscWarning { code: kUnsupportedProp_SimpleButton__url, p_warning: "For more information, see LoaderInfo.url and the Loader class." },
    AscWarning { code: kUnsupportedMeth_Date_getYear, p_warning: "Use the getFullYear() method instead." },
    AscWarning { code: kUnsupportedMeth_Date_setYear, p_warning: "Use the setFullYear() method instead." },
    AscWarning { code: kUnsupportedMeth_Date_getUTCYear, p_warning: "Use the getUTCFullYear() method instead." },
    AscWarning { code: kUnsupportedMeth_Microphone_setRate, p_warning: "Use the rate property instead." },
    AscWarning { code: kUnsupportedProp_Selection, p_warning: "The Selection class has been removed. For more information, see the addEventListener method of the class you want selection information from." },
    AscWarning { code: kUnsupportedMeth_Selection, p_warning: "The Selection class has been removed. For more information, see the addEventListener method of the class you want selection information from." },
    AscWarning { code: kUnsupportedMeth_Microphone_setGain, p_warning: "Use the gain property instead." },
    AscWarning { code: kUnsupportedMeth_ColorTransform_getTransform, p_warning: "Color values can be assigned directly using the ColorTransform class constructor or properties." },
    AscWarning { code: kUnsupportedMeth_ColorTransform_setTransform, p_warning: "Color values can be assigned directly using the ColorTransform class constructor or properties." },
    AscWarning { code: kUnsupportedProp_MovieClip_focusEnabled, p_warning: "See help for the focus related properties of the flash.display.InteractiveObject class." },
    AscWarning { code: kUnsupportedMeth_MovieClip_beginBitmapFill, p_warning: "See help for the flash.display.Graphics.beginBitmapFill method." },
    AscWarning { code: kUnsupportedMeth_MovieClip_hitTest, p_warning: "See help for the flash.display.DisplayObject.hitTestObject() method." },
    AscWarning { code: kUnsupportedMeth_MovieClip_attachBitmap, p_warning: "See help for the addChild() method." },
    AscWarning { code: kUnsupportedMeth_Sound_loadSound, p_warning: "Use the load() method instead." },
    AscWarning { code: kUnsupportedMeth_Sound_getVolume, p_warning: "Use flash.media.SoundChannel.leftPeak and flash.media.SoundChannel.rightPeak to monitor and control the amplitude of a sound channel." },
    AscWarning { code: kUnsupportedMeth_Sound_getTransform, p_warning: "Use the soundTransform property instead." },
    AscWarning { code: kUnsupportedMeth_Sound_getPan, p_warning: "Use the SoundTransform.pan property instead." },
    AscWarning { code: kUnsupportedMeth_Sound_setPan, p_warning: "Use the SoundTransform.pan property instead." },
    AscWarning { code: kUnsupportedMeth_Sound_getBytesLoaded, p_warning: "Use the bytesLoaded property instead." },
    AscWarning { code: kUnsupportedMeth_Sound_getBytesTotal, p_warning: "Use the bytesTotal property instead." },
    AscWarning { code: kWarning_SlowTextFieldAddition, p_warning: "Inefficient use of += on a TextField." },
    AscWarning { code: kWarning_SlowTextFieldAddition_specific, p_warning: "Appending text to a TextField using += is many times slower than using the TextField.appendText() method." },
    AscWarning { code: kWarning_UnlikelyFunctionValue, p_warning: "Possible missing parentheses." },
    AscWarning { code: kWarning_UnlikelyFunctionValue_specific, p_warning: "Function value used where type %s was expected.  Possibly the parentheses () are missing after this function reference." },
    AscWarning { code: kWarning_InstanceOfChanges, p_warning: "Use of the instanceof operator." },
    AscWarning { code: kWarning_InstanceOfChanges_specific, p_warning: "The instanceof operator is deprecated, use the is operator instead." },
    AscWarning { code: kUnsupportedProp_LocalConnection_allowDomain, p_warning: "The allowDomain() event handler is now a standard method, rather than an event callback. For more information, see the new LocalConnection.allowDomain method." },
    AscWarning { code: kUnsupportedProp_LocalConnection_allowInsecureDomain, p_warning: "The allowInsecureDomain() event handler is now a standard method, rather than an event callback. For more information, see the new LocalConnection.allowInsecureDomain method." },
    AscWarning { code: kUnsupportedMeth_call, p_warning: "The global call() method is no longer supported." },
    AscWarning { code: kUnsupportedProp_Color, p_warning: "The Color class has been removed. Use the flash.geom.ColorTransform class for equivalent functionality." },
    AscWarning { code: kUnsupportedMeth_Color, p_warning: "The Color class has been removed. Use the flash.geom.ColorTransform class for equivalent functionality." },
    AscWarning { code: kUnsupportedProp_System_exactSettings, p_warning: "ActionScript 3.0 SWF files always use exact domain matching rules." },
    AscWarning { code: kUnsupportedProp_capabilities, p_warning: "The capabilities class has been renamed Capabilities." },
    AscWarning { code: kUnsupportedMeth_capabilities, p_warning: "The capabilities class has been renamed Capabilities." },
    AscWarning { code: kUnsupportedMeth_FileReference_addListener, p_warning: "For more information, see addEventListener(eventName, listener, useCapture, priority )." },
    AscWarning { code: kUnsupportedMeth_FileReference_removeListener, p_warning: "For more information, see removeEventListener(eventName, listener, useCapture)." },
    AscWarning { code: kWarning_Event_onComplete, p_warning: "The onComplete event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ('complete', callback_handler)." },
    AscWarning { code: kWarning_Event_onHTTPError, p_warning: "The onHTTPError event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ('httpError', callback_handler)." },
    AscWarning { code: kWarning_Event_onIOError, p_warning: "The onIOError event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ('ioError', callback_handler)." },
    AscWarning { code: kWarning_Event_onProgress, p_warning: "The onProgress event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ('progress', callback_handler)." },
    AscWarning { code: kWarning_Event_onSecurityError, p_warning: "The onSecurityError event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ('securityError', callback_handler)." },
    AscWarning { code: kWarning_Event_onOpen, p_warning: "The onOpen event handler is not triggered automatically by Flash Player at run time in ActionScript 3.0.  You must first register this handler for the event using addEventListener ('open', callback_handler)." },
    AscWarning { code: kWarning_XML_ClassHasChanged, p_warning: "Possible usage of the ActionScript 2.0 XML class." },
    AscWarning { code: kWarning_XML_ClassHasChanged_specific, p_warning: "Migration issue: The ActionScript 2.0 XML class has been renamed XMLDocument." },
    AscWarning { code: kWarning_BadDateCast, p_warning: "Invalid Date cast operation." },
    AscWarning { code: kWarning_BadDateCast_specific, p_warning: "Date(x) behaves the same as new Date().toString(). To cast a value to type Date use \"x as Date\" instead of Date(x)." },
    AscWarning { code: kWarning_ImportHidesClass, p_warning: "Importing a package by the same name as the current class will hide that class identifier in this scope." },
    AscWarning { code: kWarning_ImportHidesClass_specific, p_warning: "Importing a package by the same name as the current class will hide that class identifier in this scope." },
    AscWarning { code: kWarning_DuplicateArgumentNames, p_warning: "More than one argument has the same name." },
    AscWarning { code: kWarning_DuplicateArgumentNames_specific, p_warning: "More than one argument named '%s' specified.  References to that argument will always resolve to the last one." },
    AscWarning { code: kUnsupportedMeth_Rectangle_containsRectangle, p_warning: "Use the Rectangle.containsRect method instead." },
    AscWarning { code: kUnsupportedMeth_eval, p_warning: "This functionality has been removed." },
    AscWarning { code: kUnsupportedMeth_getVersion, p_warning: "This functionality has been replaced by the flash.system.Capabilities.version property." },
    AscWarning { code: kUnsupportedMeth_ifFrameLoaded, p_warning: "This functionality has been replaced by the flash.display.MovieClip.framesLoaded property." },
    AscWarning { code: kUnsupportedProp__global, p_warning: "The _global property has been removed.  For equivalent functionality, use a static member of a class." },
    AscWarning { code: kWarning_BadBoolAssignment, p_warning: "Non-Boolean value used where a Boolean value was expected." },
    AscWarning { code: kWarning_BadBoolAssignment_specific, p_warning: "%s used where a Boolean value was expected.  The expression will be type coerced to Boolean." },
    AscWarning { code: kWarning_BadES3TypeProp, p_warning: "Unknown property." },
    AscWarning { code: kWarning_BadES3TypeProp_specific, p_warning: "%s is not a recognized property of the dynamic class %s." },
    AscWarning { code: kWarning_BadES3TypeMethod, p_warning: "Unknown method." },
    AscWarning { code: kWarning_BadES3TypeMethodProp_specific, p_warning: "%s is not a recognized method of the dynamic class %s." },
    AscWarning { code: kWarning_DuplicateVariableDef, p_warning: "Duplicate variable definition." },
    AscWarning { code: kWarning_DuplicateVariableDef_specific, p_warning: "Duplicate variable definition." },
    AscWarning { code: kWarning_DefinitionShadowedByPackageName, p_warning: "Definition name is the same as an imported package name.  Unqualified references to that name will resolve to the package and not the definition." },
    AscWarning { code: kWarning_DefinitionShadowedByPackageName_specific, p_warning: "Definition name is the same as an imported package name.  Unqualified references to that name will resolve to the package and not the definition." },
    AscWarning { code: kWarning_DeleteOfFixedProperty, p_warning: "Possible attempt to delete a fixed property." },
    AscWarning { code: kWarning_DeleteOfFixedProperty_specific, p_warning: "The declared property %s cannot be deleted. To free associated memory, set its value to null." },
    AscWarning { code: kWarning_Deprecated, p_warning: "Use of deprecated definition." },
    AscWarning { code: kWarning_Deprecated_specific, p_warning: "'%s' has been deprecated." },
    AscWarning { code: kWarning_DeprecatedMessage, p_warning: "Use of deprecated definition." },
    AscWarning { code: kWarning_DeprecatedMessage_specific, p_warning: "%s" },
    AscWarning { code: kWarning_DeprecatedUseReplacement, p_warning: "Use of deprecated definition." },
    AscWarning { code: kWarning_DeprecatedUseReplacement_specific, p_warning: "'%s' has been deprecated.  Please use '%s'." },
    AscWarning { code: kWarning_DeprecatedSince, p_warning: "Use of deprecated definition." },
    AscWarning { code: kWarning_DeprecatedSince_specific, p_warning: "'%s' has been deprecated since %s.  Please use '%s'." },
    AscWarning { code: kWarning_DeprecatedSinceNoReplacement, p_warning: "Use of deprecated definition." },
    AscWarning { code: kWarning_DeprecatedSinceNoReplacement_specific, p_warning: "'%s' has been deprecated since %s." },
];

pub static unsupportedProperties: [AscWarningInstance; 174] = [
    AscWarningInstance { code: kUnsupportedProp_version, base_type: kVoidType, is_static: false, name: "$version" },
    AscWarningInstance { code: kUnsupportedProp_Focusrect, base_type: kVoidType, is_static: false, name: "_focusrect" },
    AscWarningInstance { code: kUnsupportedProp__global, base_type: kVoidType, is_static: false, name: "_global" },
    AscWarningInstance { code: kUnsupportedProp_Highquality, base_type: kVoidType, is_static: false, name: "_highquality" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObj_HighQuality, base_type: kDisplayObjectType, is_static: false, name: "_highquality" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObj_Quality, base_type: kDisplayObjectType, is_static: false, name: "_quality" },
    AscWarningInstance { code: kUnsupportedProp_Quality, base_type: kVoidType, is_static: false, name: "_quality" },
    AscWarningInstance { code: kUnsupportedProp_TextField_Soundbuftime, base_type: kTextFieldType, is_static: false, name: "_soundbuftime" },
    AscWarningInstance { code: kUnsupportedProp_TextField_Target, base_type: kTextFieldType, is_static: false, name: "_target" },
    AscWarningInstance { code: kUnsupportedProp_TextField_Url, base_type: kTextFieldType, is_static: false, name: "_url" },
    AscWarningInstance { code: kUnsupportedProp_TextField_Variable, base_type: kTextFieldType, is_static: false, name: "variable" },
    AscWarningInstance { code: kUnsupportedProp_NewLine, base_type: kVoidType, is_static: false, name: "newline" },
    AscWarningInstance { code: kUnsupportedProp_MaxScroll, base_type: kVoidType, is_static: false, name: "maxscroll" },
    AscWarningInstance { code: kUnsupportedProp_Level, base_type: kVoidType, is_static: false, name: "_level" },
    AscWarningInstance { code: kUnsupportedProp_Parent, base_type: kVoidType, is_static: false, name: "_parent" },
    AscWarningInstance { code: kUnsupportedProp_Root, base_type: kVoidType, is_static: false, name: "_root" },
    AscWarningInstance { code: kUnsupportedProp_Arguments_Caller, base_type: kArrayType, is_static: false, name: "caller" },
    AscWarningInstance { code: kUnsupportedProp_Button_Target, base_type: kDisplayObjectType, is_static: false, name: "_target" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip_Parent, base_type: kDisplayObjectType, is_static: false, name: "_parent" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip_target, base_type: kDisplayObjectType, is_static: false, name: "_target" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip_hitArea, base_type: kDisplayObjectType, is_static: false, name: "hitArea" },
    AscWarningInstance { code: kUnsupportedProp_Scroll, base_type: kVoidType, is_static: false, name: "scroll" },
    AscWarningInstance { code: kUnsupportedProp_TargetPath, base_type: kVoidType, is_static: false, name: "targetPath" },
    AscWarningInstance { code: kUnsupportedProp_Video_Height, base_type: kVideoType, is_static: false, name: "height" },
    AscWarningInstance { code: kUnsupportedProp_Video_Width, base_type: kVideoType, is_static: false, name: "width" },
    AscWarningInstance { code: kUnsupportedProp__Proto__, base_type: kObjectType, is_static: false, name: "__proto__" },
    AscWarningInstance { code: kUnsupportedProp_Stage, base_type: kVoidType, is_static: false, name: "Stage" },
    AscWarningInstance { code: kUnsupportedProp__remoteClass, base_type: kObjectType, is_static: false, name: "_remoteClass" },
    AscWarningInstance { code: kUnsupportedProp_ActivityEvent_ACTIVITY_TYPE, base_type: kActivityEventType, is_static: true, name: "ACTIVITY_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Camera_currentFps, base_type: kCameraType, is_static: false, name: "currentFps" },
    AscWarningInstance { code: kUnsupportedProp_ContextMenu_forward_back, base_type: kContextMenuType, is_static: false, name: "forward_back" },
    AscWarningInstance { code: kUnsupportedProp_DataEvent_DATA_TYPE, base_type: kDataEventType, is_static: true, name: "DATA_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_xscale, base_type: kDisplayObjectType, is_static: false, name: "xscale" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__xscale, base_type: kDisplayObjectType, is_static: false, name: "_xscale" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_yscale, base_type: kDisplayObjectType, is_static: false, name: "yscale" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__yscale, base_type: kDisplayObjectType, is_static: false, name: "_yscale" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_xmouse, base_type: kDisplayObjectType, is_static: false, name: "xmouse" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__xmouse, base_type: kDisplayObjectType, is_static: false, name: "_xmouse" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_ymouse, base_type: kDisplayObjectType, is_static: false, name: "ymouse" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__ymouse, base_type: kDisplayObjectType, is_static: false, name: "_ymouse" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_setRootClass, base_type: kDisplayObjectType, is_static: false, name: "setRootClass" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_mapSymbolToClass, base_type: kDisplayObjectType, is_static: false, name: "mapSymbolToClass" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__name, base_type: kDisplayObjectType, is_static: false, name: "_name" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__parent, base_type: kDisplayObjectType, is_static: false, name: "_parent" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__mask, base_type: kDisplayObjectType, is_static: false, name: "_mask" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__visible, base_type: kDisplayObjectType, is_static: false, name: "_visible" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__x, base_type: kDisplayObjectType, is_static: false, name: "_x" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__y, base_type: kDisplayObjectType, is_static: false, name: "_y" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__rotation, base_type: kDisplayObjectType, is_static: false, name: "_rotation" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__alpha, base_type: kDisplayObjectType, is_static: false, name: "_alpha" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__width, base_type: kDisplayObjectType, is_static: false, name: "_width" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject__height, base_type: kDisplayObjectType, is_static: false, name: "_height" },
    AscWarningInstance { code: kUnsupportedProp_ErrorEvent_ERROR_TYPE, base_type: kErrorEventType, is_static: true, name: "ERROR_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_ACTIVATE_TYPE, base_type: kEventType, is_static: true, name: "ACTIVATE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_ADDED_TYPE, base_type: kEventType, is_static: true, name: "ADDED_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_CANCEL_TYPE, base_type: kEventType, is_static: true, name: "CANCEL_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_CHANGE_TYPE, base_type: kEventType, is_static: true, name: "CHANGE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_CLOSE_TYPE, base_type: kEventType, is_static: true, name: "CLOSE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_COMPLETE_TYPE, base_type: kEventType, is_static: true, name: "COMPLETE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_CONNECT_TYPE, base_type: kEventType, is_static: true, name: "CONNECT_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_DEACTIVATE_TYPE, base_type: kEventType, is_static: true, name: "DEACTIVATE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_ENTER_FRAME_TYPE, base_type: kEventType, is_static: true, name: "ENTER_FRAME_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_ID3_TYPE, base_type: kEventType, is_static: true, name: "ID3_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_INIT_TYPE, base_type: kEventType, is_static: true, name: "INIT_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_MOUSE_LEAVE_TYPE, base_type: kEventType, is_static: true, name: "MOUSE_LEAVE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_OPEN_TYPE, base_type: kEventType, is_static: true, name: "OPEN_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_REMOVED_TYPE, base_type: kEventType, is_static: true, name: "REMOVED_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_RENDER_TYPE, base_type: kEventType, is_static: true, name: "RENDER_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_RESIZE_TYPE, base_type: kEventType, is_static: true, name: "RESIZE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_SCROLL_TYPE, base_type: kEventType, is_static: true, name: "SCROLL_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_SELECT_TYPE, base_type: kEventType, is_static: true, name: "SELECT_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_SOUND_COMPLETE_TYPE, base_type: kEventType, is_static: true, name: "SOUND_COMPLETE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_TAB_CHILDREN_CHANGE_TYPE, base_type: kEventType, is_static: true, name: "TAB_CHILDREN_CHANGE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_TAB_ENABLED_CHANGE_TYPE, base_type: kEventType, is_static: true, name: "TAB_ENABLED_CHANGE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_TAB_INDEX_CHANGE_TYPE, base_type: kEventType, is_static: true, name: "TAB_INDEX_CHANGE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_Event_UNLOAD_TYPE, base_type: kEventType, is_static: true, name: "UNLOAD_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_FocusEvent_FOCUS_IN_TYPE, base_type: kFocusEventType, is_static: true, name: "FOCUS_IN_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_FocusEvent_FOCUS_OUT_TYPE, base_type: kFocusEventType, is_static: true, name: "FOCUS_OUT_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_FocusEvent_KEY_FOCUS_CHANGE_TYPE, base_type: kFocusEventType, is_static: true, name: "KEY_FOCUS_CHANGE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_FocusEvent_MOUSE_FOCUS_CHANGE_TYPE, base_type: kFocusEventType, is_static: true, name: "MOUSE_FOCUS_CHANGE_TYPE" },
    AscWarningInstance { code: kUnsupportedProp_BitmapFilter_XYZ_QUALITY, base_type: kBitmapFilterType, is_static: false, name: "XYZ_QUALITY" },
    AscWarningInstance { code: kUnsupportedProp_Keyboard_PGDN, base_type: kKeyboardType, is_static: true, name: "PGDN" },
    AscWarningInstance { code: kUnsupportedProp_Keyboard_PGUP, base_type: kKeyboardType, is_static: true, name: "PGUP" },
    AscWarningInstance { code: kUnsupportedProp_Keyboard_DELETEKEY, base_type: kKeyboardType, is_static: true, name: "DELETEKEY" },
    AscWarningInstance { code: kUnsupportedProp_Keyboard_CAPSLOCK, base_type: kKeyboardType, is_static: true, name: "CAPSLOCK" },
    AscWarningInstance { code: kUnsupportedProp_Keyboard_NUMPAD, base_type: kKeyboardType, is_static: true, name: "NUMPAD" },
    AscWarningInstance { code: kUnsupportedProp_InteractiveObject__focusrect, base_type: kInteractiveObjectType, is_static: false, name: "_focusrect" },
    AscWarningInstance { code: kUnsupportedProp_InteractiveObject_menu, base_type: kInteractiveObjectType, is_static: false, name: "menu" },
    AscWarningInstance { code: kUnsupportedProp_KeyboardEvent_ascii, base_type: kKeyboardEventType, is_static: false, name: "ascii" },
    AscWarningInstance { code: kUnsupportedProp_KeyboardEvent_code, base_type: kKeyboardEventType, is_static: false, name: "code" },
    AscWarningInstance { code: kUnsupportedProp_Loader_loaderInfo, base_type: kLoaderType, is_static: false, name: "loaderInfo" },
    AscWarningInstance { code: kUnsupportedProp_Loader_preload, base_type: kLoaderType, is_static: false, name: "preload" },
    AscWarningInstance { code: kUnsupportedProp_Loader_loadCached, base_type: kLoaderType, is_static: false, name: "loadCached" },
    AscWarningInstance { code: kUnsupportedProp_LoaderInfo_loaderUrl, base_type: kLoaderInfoType, is_static: false, name: "loaderUrl" },
    AscWarningInstance { code: kUnsupportedProp_ContextMenuEvent_menuOwner, base_type: kContextMenuEventType, is_static: false, name: "menuOwner" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip_url, base_type: kMovieClipType, is_static: false, name: "url" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip__url, base_type: kMovieClipType, is_static: false, name: "_url" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip__currentframe, base_type: kMovieClipType, is_static: false, name: "_currentframe" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip__framesloaded, base_type: kMovieClipType, is_static: false, name: "_framesloaded" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip__totalframes, base_type: kMovieClipType, is_static: false, name: "_totalframes" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip_lockroot, base_type: kMovieClipType, is_static: false, name: "lockroot" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip__lockroot, base_type: kMovieClipType, is_static: false, name: "_lockroot" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip__soundbuftime, base_type: kMovieClipType, is_static: false, name: "_soundbuftime" },
    AscWarningInstance { code: kUnsupportedProp_NetStream_currentFps, base_type: kNetStreamType, is_static: false, name: "currentFps" },
    AscWarningInstance { code: kUnsupportedProp_NetStream_videocodec, base_type: kNetStreamType, is_static: false, name: "videocodec" },
    AscWarningInstance { code: kUnsupportedProp_NetStream_audiocodec, base_type: kNetStreamType, is_static: false, name: "audiocodec" },
    AscWarningInstance { code: kUnsupportedProp_Proxy_delDescendants, base_type: kProxyType, is_static: false, name: "delDescendants" },
    AscWarningInstance { code: kUnsupportedProp_ProgressEvent_current, base_type: kProgressEventType, is_static: false, name: "current" },
    AscWarningInstance { code: kUnsupportedProp_ProgressEvent_total, base_type: kProgressEventType, is_static: false, name: "total" },
    AscWarningInstance { code: kUnsupportedProp_Sprite__droptarget, base_type: kSpriteType, is_static: false, name: "_droptarget" },
    AscWarningInstance { code: kUnsupportedProp_Stage_showMenu, base_type: kStageType, is_static: false, name: "showMenu" },
    AscWarningInstance { code: kUnsupportedProp_StyleSheet_loaded, base_type: kStyleSheetType, is_static: false, name: "loaded" },
    AscWarningInstance { code: kUnsupportedProp_System_getAvmplusVersion, base_type: kSystemType, is_static: true, name: "getAvmplusVersion" },
    AscWarningInstance { code: kUnsupportedProp_SWFLoaderInfo_SWFVersion, base_type: kSWFLoaderInfoType, is_static: false, name: "SWFVersion" },
    AscWarningInstance { code: kUnsupportedProp_SWFLoaderInfo_ASVersion, base_type: kSWFLoaderInfoType, is_static: false, name: "ASVersion" },
    AscWarningInstance { code: kUnsupportedProp_TextField_maxscroll, base_type: kTextFieldType, is_static: false, name: "maxscroll" },
    AscWarningInstance { code: kUnsupportedProp_TextField_hscroll, base_type: kTextFieldType, is_static: false, name: "hscroll" },
    AscWarningInstance { code: kUnsupportedProp_TextField_maxhscroll, base_type: kTextFieldType, is_static: false, name: "maxhscroll" },
    AscWarningInstance { code: kUnsupportedProp_TextField_newTextFormat, base_type: kTextFieldType, is_static: false, name: "newTextFormat" },
    AscWarningInstance { code: kUnsupportedProp_URLLoader_navigate, base_type: kURLLoaderType, is_static: false, name: "navigate" },
    AscWarningInstance { code: kUnsupportedProp_URLLoader_binary, base_type: kURLLoaderType, is_static: false, name: "binary" },
    AscWarningInstance { code: kUnsupportedProp_URLRequest_digest, base_type: kURLRequestType, is_static: false, name: "digest" },
    AscWarningInstance { code: kUnsupportedProp_URLRequest_importToSandbox, base_type: kURLRequestType, is_static: false, name: "importToSandbox" },
    AscWarningInstance { code: kUnsupportedProp_XMLDocument_loaded, base_type: kXMLDocumentType, is_static: false, name: "loaded" },
    AscWarningInstance { code: kUnsupportedProp_XMLDocument_contentType, base_type: kXMLDocumentType, is_static: false, name: "contentType" },
    AscWarningInstance { code: kUnsupportedProp_XMLDocument_status, base_type: kXMLDocumentType, is_static: false, name: "status" },
    AscWarningInstance { code: kUnsupportedProp_Button, base_type: kVoidType, is_static: false, name: "Button" },
    AscWarningInstance { code: kUnsupportedProp_Container, base_type: kVoidType, is_static: false, name: "Container" },
    AscWarningInstance { code: kUnsupportedProp_Image, base_type: kVoidType, is_static: false, name: "Image" },
    AscWarningInstance { code: kUnsupportedProp_ImageFilter, base_type: kVoidType, is_static: false, name: "ImageFilter" },
    AscWarningInstance { code: kUnsupportedProp_ImageSprite, base_type: kVoidType, is_static: false, name: "ImageSprite" },
    AscWarningInstance { code: kUnsupportedProp_ImageLoaderInfo, base_type: kVoidType, is_static: false, name: "ImageLoaderInfo" },
    AscWarningInstance { code: kUnsupportedProp_ImeEvent, base_type: kVoidType, is_static: false, name: "ImeEvent" },
    AscWarningInstance { code: kUnsupportedProp_Key, base_type: kVoidType, is_static: false, name: "Key" },
    AscWarningInstance { code: kUnsupportedProp_LineMetrics, base_type: kVoidType, is_static: false, name: "LineMetrics" },
    AscWarningInstance { code: kUnsupportedProp_LoadVars, base_type: kVoidType, is_static: false, name: "LoadVars" },
    AscWarningInstance { code: kUnsupportedProp_MenuEvent, base_type: kVoidType, is_static: false, name: "MenuEvent" },
    AscWarningInstance { code: kUnsupportedProp_MovieClipLoader, base_type: kVoidType, is_static: false, name: "MovieClipLoader" },
    AscWarningInstance { code: kUnsupportedProp_SystemCapabilities, base_type: kVoidType, is_static: false, name: "SystemCapabilities" },
    AscWarningInstance { code: kUnsupportedProp_SystemCapabilities, base_type: kVoidType, is_static: false, name: "SystemCapabilities" },
    AscWarningInstance { code: kUnsupportedProp_TextExtents, base_type: kVoidType, is_static: false, name: "TextExtents" },
    AscWarningInstance { code: kUnsupportedProp_TextExtents, base_type: kVoidType, is_static: false, name: "TextExtents" },
    AscWarningInstance { code: kUnsupportedProp_DisplayObject_accProps, base_type: kDisplayObjectType, is_static: false, name: "accProps" },
    AscWarningInstance { code: kUnsupportedProp_NetConnection_isConnected, base_type: kNetConnectionType, is_static: false, name: "isConnected" },
    AscWarningInstance { code: kUnsupportedProp_Socket_isConnected, base_type: kSocketType, is_static: false, name: "isConnected" },
    AscWarningInstance { code: kUnsupportedProp_URLStream_isConnected, base_type: kURLStreamType, is_static: false, name: "isConnected" },
    AscWarningInstance { code: kUnsupportedProp_SyncEvent_list, base_type: kSyncEventType, is_static: false, name: "list" },
    AscWarningInstance { code: kUnsupportedProp_TextField_scroll, base_type: kTextFieldType, is_static: false, name: "scroll" },
    AscWarningInstance { code: kUnsupportedProp_TextField_bottomScroll, base_type: kTextFieldType, is_static: false, name: "bottomScroll" },
    AscWarningInstance { code: kUnsupportedProp_BitmapData_RED_CHANNEL, base_type: kBitmapDataType, is_static: false, name: "RED_CHANNEL" },
    AscWarningInstance { code: kUnsupportedProp_BitmapData_GREEN_CHANNEL, base_type: kBitmapDataType, is_static: false, name: "GREEN_CHANNEL" },
    AscWarningInstance { code: kUnsupportedProp_BitmapData_BLUE_CHANNEL, base_type: kBitmapDataType, is_static: false, name: "BLUE_CHANNEL" },
    AscWarningInstance { code: kUnsupportedProp_BitmapData_ALPHA_CHANNEL, base_type: kBitmapDataType, is_static: false, name: "ALPHA_CHANNEL" },
    AscWarningInstance { code: kUnsupportedProp_System_useCodepage, base_type: kSystemType, is_static: true, name: "useCodepage" },
    AscWarningInstance { code: kUnsupportedProp_SimpleButton_Soundbuftime, base_type: kSimpleButtonType, is_static: false, name: "_soundbuftime" },
    AscWarningInstance { code: kUnsupportedProp_AsBroadcaster, base_type: kVoidType, is_static: false, name: "AsBroadcaster" },
    AscWarningInstance { code: kUnsupportedProp_IME_ALPHANUMERIC_FULL, base_type: kIMEType, is_static: true, name: "ALPHANUMERIC_FULL" },
    AscWarningInstance { code: kUnsupportedProp_IME_ALPHANUMERIC_HALF, base_type: kIMEType, is_static: true, name: "ALPHANUMERIC_HALF" },
    AscWarningInstance { code: kUnsupportedProp_IME_CHINESE, base_type: kIMEType, is_static: true, name: "CHINESE" },
    AscWarningInstance { code: kUnsupportedProp_IME_JAPANESE_HIRAGANA, base_type: kIMEType, is_static: true, name: "JAPANESE_HIRAGANA" },
    AscWarningInstance { code: kUnsupportedProp_IME_JAPANESE_KATAKANA_FULL, base_type: kIMEType, is_static: true, name: "JAPANESE_KATAKANA_FULL" },
    AscWarningInstance { code: kUnsupportedProp_IME_JAPANESE_KATAKANA_HALF, base_type: kIMEType, is_static: true, name: "JAPANESE_KATAKANA_HALF" },
    AscWarningInstance { code: kUnsupportedProp_IME_KOREAN, base_type: kIMEType, is_static: true, name: "KOREAN" },
    AscWarningInstance { code: kUnsupportedProp_IME_UNKNOWN, base_type: kIMEType, is_static: true, name: "UNKNOWN" },
    AscWarningInstance { code: kUnsupportedProp_Keyboard__listeners, base_type: kKeyboardType, is_static: false, name: "_listeners" },
    AscWarningInstance { code: kUnsupportedProp_SimpleButton__url, base_type: kSimpleButtonType, is_static: false, name: "_url" },
    AscWarningInstance { code: kUnsupportedProp_Selection, base_type: kVoidType, is_static: false, name: "Selection" },
    AscWarningInstance { code: kUnsupportedProp_MovieClip_focusEnabled, base_type: kDisplayObjectType, is_static: false, name: "focusEnabled" },
    AscWarningInstance { code: kUnsupportedProp_LocalConnection_allowDomain, base_type: kLocalConnectionType, is_static: false, name: "allowDomain" },
    AscWarningInstance { code: kUnsupportedProp_LocalConnection_allowInsecureDomain, base_type: kLocalConnectionType, is_static: false, name: "allowInsecureDomain" },
    AscWarningInstance { code: kUnsupportedProp_Color, base_type: kVoidType, is_static: false, name: "Color" },
    AscWarningInstance { code: kUnsupportedProp_System_exactSettings, base_type: kSystemType, is_static: false, name: "exactSettings" },
    AscWarningInstance { code: kUnsupportedProp_capabilities, base_type: kVoidType, is_static: false, name: "capabilities" },
    AscWarningInstance { code: kUnsupportedProp_Object___resolve, base_type: kObjectType, is_static: false, name: "__resolve" },
];

pub static unsupportedMethods: [AscWarningInstance; 218] = [
    AscWarningInstance { code: kUnsupportedMeth_instanceof, base_type: kVoidType, is_static: false, name: "instanceof" },
    AscWarningInstance { code: kUnsupportedMeth_DisplayObject_setScalingGrid, base_type: kDisplayObjectType, is_static: false, name: "setScalingGrid" },
    AscWarningInstance { code: kUnsupportedMeth_Graphics_drawCircle, base_type: kGraphicsType, is_static: false, name: "drawCircle" },
    AscWarningInstance { code: kUnsupportedMeth_XMLUI_get, base_type: kXMLUIType, is_static: false, name: "get" },
    AscWarningInstance { code: kUnsupportedMeth_XMLUI_set, base_type: kXMLUIType, is_static: false, name: "set" },
    AscWarningInstance { code: kUnsupportedMeth_hasChildNodes, base_type: kXMLNodeType, is_static: false, name: "hasChildNodes" },
    AscWarningInstance { code: kUnsupportedMeth_XMLEvent, base_type: kVoidType, is_static: false, name: "XMLEvent" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDoc, base_type: kVoidType, is_static: false, name: "XMLDoc" },
    AscWarningInstance { code: kUnsupportedMeth_random, base_type: kVoidType, is_static: false, name: "random" },
    AscWarningInstance { code: kUnsupportedMeth_chr, base_type: kVoidType, is_static: false, name: "chr" },
    AscWarningInstance { code: kUnsupportedMeth_mbchr, base_type: kVoidType, is_static: false, name: "mbchr" },
    AscWarningInstance { code: kUnsupportedMeth_ord, base_type: kVoidType, is_static: false, name: "ord" },
    AscWarningInstance { code: kUnsupportedMeth_mbord, base_type: kVoidType, is_static: false, name: "mbord" },
    AscWarningInstance { code: kUnsupportedMeth_substring, base_type: kVoidType, is_static: false, name: "substring" },
    AscWarningInstance { code: kUnsupportedMeth_mbsubstring, base_type: kVoidType, is_static: false, name: "mbsubstring" },
    AscWarningInstance { code: kUnsupportedMeth_length, base_type: kVoidType, is_static: false, name: "length" },
    AscWarningInstance { code: kUnsupportedMeth_mblength, base_type: kVoidType, is_static: false, name: "mblength" },
    AscWarningInstance { code: kUnsupportedMeth_ASNative, base_type: kVoidType, is_static: false, name: "ASnative" },
    AscWarningInstance { code: kUnsupportedMeth_addProperty, base_type: kObjectType, is_static: false, name: "addProperty" },
    AscWarningInstance { code: kUnsupportedMeth_getProperty, base_type: kVoidType, is_static: false, name: "getProperty" },
    AscWarningInstance { code: kUnsupportedMeth_setProperty, base_type: kVoidType, is_static: false, name: "setProperty" },
    AscWarningInstance { code: kUnsupportedMeth_asfunction, base_type: kVoidType, is_static: false, name: "asfunction" },
    AscWarningInstance { code: kUnsupportedMeth_clearInterval, base_type: kVoidType, is_static: false, name: "clearInterval" },
    AscWarningInstance { code: kUnsupportedMeth_duplicateMovieClip, base_type: kVoidType, is_static: false, name: "duplicateMovieClip" },
    AscWarningInstance { code: kUnsupportedMeth_fscommand, base_type: kVoidType, is_static: false, name: "fscommand" },
    AscWarningInstance { code: kUnsupportedMeth_getURL, base_type: kVoidType, is_static: false, name: "getURL" },
    AscWarningInstance { code: kUnsupportedMeth_gotoAndPlay, base_type: kVoidType, is_static: false, name: "gotoAndPlay" },
    AscWarningInstance { code: kUnsupportedMeth_gotoAndStop, base_type: kVoidType, is_static: false, name: "gotoAndStop" },
    AscWarningInstance { code: kUnsupportedMeth_play, base_type: kVoidType, is_static: false, name: "play" },
    AscWarningInstance { code: kUnsupportedMeth_print, base_type: kVoidType, is_static: false, name: "print" },
    AscWarningInstance { code: kUnsupportedMeth_printAsBitmap, base_type: kVoidType, is_static: false, name: "printAsBitmap" },
    AscWarningInstance { code: kUnsupportedMeth_printAsBitmapNum, base_type: kVoidType, is_static: false, name: "printAsBitmapNum" },
    AscWarningInstance { code: kUnsupportedMeth_printNum, base_type: kVoidType, is_static: false, name: "printNum" },
    AscWarningInstance { code: kUnsupportedMeth_removeMovieClip, base_type: kVoidType, is_static: false, name: "removeMovieClip" },
    AscWarningInstance { code: kUnsupportedMeth_setInterval, base_type: kVoidType, is_static: false, name: "setInterval" },
    AscWarningInstance { code: kUnsupportedMeth_nextFrame, base_type: kVoidType, is_static: false, name: "nextFrame" },
    AscWarningInstance { code: kUnsupportedMeth_startDrag, base_type: kVoidType, is_static: false, name: "startDrag" },
    AscWarningInstance { code: kUnsupportedMeth_stop, base_type: kVoidType, is_static: false, name: "stop" },
    AscWarningInstance { code: kUnsupportedMeth_stopAllSounds, base_type: kVoidType, is_static: false, name: "stopAllSounds" },
    AscWarningInstance { code: kUnsupportedMeth_stopDrag, base_type: kVoidType, is_static: false, name: "stopDrag" },
    AscWarningInstance { code: kUnsupportedMeth_tellTarget, base_type: kVoidType, is_static: false, name: "tellTarget" },
    AscWarningInstance { code: kUnsupportedMeth_toggleHighQuality, base_type: kVoidType, is_static: false, name: "toggleHighQuality" },
    AscWarningInstance { code: kUnsupportedMeth_unloadMovie, base_type: kVoidType, is_static: false, name: "unloadMovie" },
    AscWarningInstance { code: kUnsupportedMeth_unloadMovieNum, base_type: kVoidType, is_static: false, name: "unloadMovieNum" },
    AscWarningInstance { code: kUnsupportedMeth_updateAfterEvent, base_type: kVoidType, is_static: false, name: "updateAfterEvent" },
    AscWarningInstance { code: kUnsupportedMeth_Video_attachVideo, base_type: kVideoType, is_static: false, name: "attachVideo" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_StyleSheet_load, base_type: kStyleSheetType, is_static: false, name: "load" },
    AscWarningInstance { code: kUnsupportedMeth_Object_registerClass, base_type: kObjectType, is_static: false, name: "registerClass" },
    AscWarningInstance { code: kUnsupportedMeth_Object_unwatch, base_type: kObjectType, is_static: false, name: "unwatch" },
    AscWarningInstance { code: kUnsupportedMeth_Object_watch, base_type: kObjectType, is_static: false, name: "watch" },
    AscWarningInstance { code: kUnsupportedMeth_loadMovie, base_type: kVoidType, is_static: false, name: "loadMovie" },
    AscWarningInstance { code: kUnsupportedMeth_loadMovieNum, base_type: kVoidType, is_static: false, name: "loadMovieNum" },
    AscWarningInstance { code: kUnsupportedMeth_loadVariables, base_type: kVoidType, is_static: false, name: "loadVariables" },
    AscWarningInstance { code: kUnsupportedMeth_loadVariablesNum, base_type: kVoidType, is_static: false, name: "loadVariablesNum" },
    AscWarningInstance { code: kUnsupportedMeth_Stage_addListener, base_type: kStageType, is_static: false, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_Stage_removeListener, base_type: kStageType, is_static: false, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_addListener, base_type: kTextFieldType, is_static: false, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_removeListener, base_type: kTextFieldType, is_static: false, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_Mouse_addListener, base_type: kMouseType, is_static: false, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_Mouse_removeListener, base_type: kMouseType, is_static: false, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_Keyboard_addListener, base_type: kKeyboardType, is_static: true, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_Keyboard_removeListener, base_type: kKeyboardType, is_static: true, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_attachSound, base_type: kSoundType, is_static: false, name: "attachSound" },
    AscWarningInstance { code: kUnsupportedMeth_Accessibility_isActive, base_type: kAccessibilityType, is_static: false, name: "isActive" },
    AscWarningInstance { code: kUnsupportedMeth_SimpleButton_getDepth, base_type: kSimpleButtonType, is_static: false, name: "getDepth" },
    AscWarningInstance { code: kUnsupportedMeth_SimpleButton_swapDepths, base_type: kSimpleButtonType, is_static: false, name: "swapDepths" },
    AscWarningInstance { code: kUnsupportedMeth_SimpleButton_getInstanceAtDepth, base_type: kSimpleButtonType, is_static: false, name: "getInstanceAtDepth" },
    AscWarningInstance { code: kUnsupportedMeth_SimpleButton_getNextHighestDepth, base_type: kSimpleButtonType, is_static: false, name: "getNextHighestDepth" },
    AscWarningInstance { code: kUnsupportedMeth_ByteArray_available, base_type: kByteArrayType, is_static: false, name: "available" },
    AscWarningInstance { code: kUnsupportedMeth_ByteArray_getFilePointer, base_type: kByteArrayType, is_static: false, name: "getFilePointer" },
    AscWarningInstance { code: kUnsupportedMeth_ByteArray_seek, base_type: kByteArrayType, is_static: false, name: "seek" },
    AscWarningInstance { code: kUnsupportedMeth_Camera_get, base_type: kCameraType, is_static: false, name: "get" },
    AscWarningInstance { code: kUnsupportedMeth_Camera_setKeyFrameInterval, base_type: kCameraType, is_static: false, name: "setKeyFrameInterval" },
    AscWarningInstance { code: kUnsupportedMeth_Camera_setLoopback, base_type: kCameraType, is_static: false, name: "setLoopback" },
    AscWarningInstance { code: kUnsupportedMeth_ColorTransform_getRGB, base_type: kColorTransformType, is_static: false, name: "getRGB" },
    AscWarningInstance { code: kUnsupportedMeth_ColorTransform_setRGB, base_type: kColorTransformType, is_static: false, name: "setRGB" },
    AscWarningInstance { code: kUnsupportedMeth_DisplayObjectContainer_getTextSnapshot, base_type: kDisplayObjectContainerType, is_static: false, name: "getTextSnapshot" },
    AscWarningInstance { code: kUnsupportedMeth_ContextMenu_copy, base_type: kContextMenuType, is_static: false, name: "copy" },
    AscWarningInstance { code: kUnsupportedMeth_ContextMenuItem_copy, base_type: kContextMenuItemType, is_static: false, name: "copy" },
    AscWarningInstance { code: kUnsupportedMeth_CustomActions_listActions, base_type: kCustomActionsType, is_static: false, name: "listActions" },
    AscWarningInstance { code: kUnsupportedMeth_ExternalInterface_available, base_type: kExternalInterfaceType, is_static: false, name: "available" },
    AscWarningInstance { code: kUnsupportedMeth_Event_isDefaultPrevented, base_type: kEventType, is_static: false, name: "isDefaultPrevented" },
    AscWarningInstance { code: kUnsupportedMeth_Graphics_beginImageFill, base_type: kGraphicsType, is_static: false, name: "beginImageFill" },
    AscWarningInstance { code: kUnsupportedMeth_Keyboard_getAscii, base_type: kKeyboardType, is_static: false, name: "getAscii" },
    AscWarningInstance { code: kUnsupportedMeth_Keyboard_getCode, base_type: kKeyboardType, is_static: false, name: "getCode" },
    AscWarningInstance { code: kUnsupportedMeth_Keyboard_isDown, base_type: kKeyboardType, is_static: false, name: "isDown" },
    AscWarningInstance { code: kUnsupportedMeth_Keyboard_isToggled, base_type: kKeyboardType, is_static: false, name: "isToggled" },
    AscWarningInstance { code: kUnsupportedMeth_LocalConnection_domain, base_type: kLocalConnectionType, is_static: false, name: "domain" },
    AscWarningInstance { code: kUnsupportedMeth_Microphone_get, base_type: kMicrophoneType, is_static: false, name: "get" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_attachMovie, base_type: kMovieClipType, is_static: false, name: "attachMovie" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_createEmptyMovieClip, base_type: kMovieClipType, is_static: false, name: "createEmptyMovieClip" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_createTextField, base_type: kMovieClipType, is_static: false, name: "createTextField" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_removeMovieClip, base_type: kMovieClipType, is_static: false, name: "removeMovieClip" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_loadMovie, base_type: kMovieClipType, is_static: false, name: "loadMovie" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_unloadMovie, base_type: kMovieClipType, is_static: false, name: "unloadMovie" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_unloadMovieNum, base_type: kMovieClipType, is_static: false, name: "unloadMovieNum" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getDepth, base_type: kMovieClipType, is_static: false, name: "getDepth" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_swapDepths, base_type: kMovieClipType, is_static: false, name: "swapDepths" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getInstanceAtDepth, base_type: kMovieClipType, is_static: false, name: "getInstanceAtDepth" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getNextHighestDepth, base_type: kMovieClipType, is_static: false, name: "getNextHighestDepth" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_attachAudio, base_type: kMovieClipType, is_static: false, name: "attachAudio" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getBytesLoaded, base_type: kMovieClipType, is_static: false, name: "getBytesLoaded" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getBytesTotal, base_type: kMovieClipType, is_static: false, name: "getBytesTotal" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getURL, base_type: kMovieClipType, is_static: false, name: "getURL" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_setMask, base_type: kMovieClipType, is_static: false, name: "setMask" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_getSWFVersion, base_type: kMovieClipType, is_static: false, name: "getSWFVersion" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_clear, base_type: kMovieClipType, is_static: false, name: "clear" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_beginFill, base_type: kMovieClipType, is_static: false, name: "beginFill" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_beginGradientFill, base_type: kMovieClipType, is_static: false, name: "beginGradientFill" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_lineGradientStyle, base_type: kMovieClipType, is_static: false, name: "lineGradientStyle" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_beginImageFill, base_type: kMovieClipType, is_static: false, name: "beginImageFill" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_lineStyle, base_type: kMovieClipType, is_static: false, name: "lineStyle" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_drawRect, base_type: kMovieClipType, is_static: false, name: "drawRect" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_drawRoundRect, base_type: kMovieClipType, is_static: false, name: "drawRoundRect" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_drawRoundRectComplex, base_type: kMovieClipType, is_static: false, name: "drawRoundRectComplex" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_drawCircle, base_type: kMovieClipType, is_static: false, name: "drawCircle" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_moveTo, base_type: kMovieClipType, is_static: false, name: "moveTo" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_lineTo, base_type: kMovieClipType, is_static: false, name: "lineTo" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_curveTo, base_type: kMovieClipType, is_static: false, name: "curveTo" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_endFill, base_type: kMovieClipType, is_static: false, name: "endFill" },
    AscWarningInstance { code: kUnsupportedMeth_NetStream_setBufferTime, base_type: kNetStreamType, is_static: false, name: "setBufferTime" },
    AscWarningInstance { code: kUnsupportedMeth_ProductManager_isIntalled, base_type: kProductManagerType, is_static: false, name: "isIntalled" },
    AscWarningInstance { code: kUnsupportedMeth_ProductManager_installedVersion, base_type: kProductManagerType, is_static: false, name: "installedVersion" },
    AscWarningInstance { code: kUnsupportedMeth_ProductManager_isRunning, base_type: kProductManagerType, is_static: false, name: "isRunning" },
    AscWarningInstance { code: kUnsupportedMeth_Point_addTo, base_type: kPointType, is_static: false, name: "addTo" },
    AscWarningInstance { code: kUnsupportedMeth_Profiler_heapdump, base_type: kVoidType, is_static: false, name: "heapdump" },
    AscWarningInstance { code: kUnsupportedMeth_Rectangle_isEmpty, base_type: kRectangleType, is_static: false, name: "isEmpty" },
    AscWarningInstance { code: kUnsupportedMeth_Rectangle_containsRectangle, base_type: kRectangleType, is_static: false, name: "containsRectangle" },
    AscWarningInstance { code: kUnsupportedMeth_SoundTransform_setPan, base_type: kSoundTransformType, is_static: false, name: "setPan" },
    AscWarningInstance { code: kUnsupportedMeth_Socket_available, base_type: kSocketType, is_static: false, name: "available" },
    AscWarningInstance { code: kUnsupportedMeth_SharedObject_getSize, base_type: kSharedObjectType, is_static: false, name: "getSize" },
    AscWarningInstance { code: kUnsupportedMeth_SharedObject_setFps, base_type: kSharedObjectType, is_static: false, name: "setFps" },
    AscWarningInstance { code: kUnsupportedMeth_Sprite_getSWF, base_type: kSpriteType, is_static: false, name: "getSWF" },
    AscWarningInstance { code: kUnsupportedMeth_Sprite_constructChild, base_type: kSpriteType, is_static: false, name: "constructChild" },
    AscWarningInstance { code: kUnsupportedMeth_Stage_getFocus, base_type: kStageType, is_static: false, name: "getFocus" },
    AscWarningInstance { code: kUnsupportedMeth_Stage_setFocus, base_type: kStageType, is_static: false, name: "setFocus" },
    AscWarningInstance { code: kUnsupportedMeth_StyleSheet_getStyleNames, base_type: kStyleSheetType, is_static: false, name: "getStyleNames" },
    AscWarningInstance { code: kUnsupportedMeth_StyleSheet_onData, base_type: kStyleSheetType, is_static: false, name: "onData" },
    AscWarningInstance { code: kUnsupportedMeth_StyleSheet_load, base_type: kStyleSheetType, is_static: false, name: "load" },
    AscWarningInstance { code: kUnsupportedMeth_StyleSheet_getBytesLoaded, base_type: kStyleSheetType, is_static: false, name: "getBytesLoaded" },
    AscWarningInstance { code: kUnsupportedMeth_StyleSheet_getBytesTotal, base_type: kStyleSheetType, is_static: false, name: "getBytesTotal" },
    AscWarningInstance { code: kUnsupportedMeth_IME_addListener, base_type: kIMEType, is_static: true, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_IME_removeListener, base_type: kIMEType, is_static: true, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_IME_getEnabled, base_type: kIMEType, is_static: false, name: "getEnabled" },
    AscWarningInstance { code: kUnsupportedMeth_IME_setEnabled, base_type: kIMEType, is_static: false, name: "setEnabled" },
    AscWarningInstance { code: kUnsupportedMeth_IME_getInstance, base_type: kIMEType, is_static: false, name: "getInstance" },
    AscWarningInstance { code: kUnsupportedMeth_IME_getConversionMode, base_type: kIMEType, is_static: false, name: "getConversionMode" },
    AscWarningInstance { code: kUnsupportedMeth_IME_setConversionMode, base_type: kIMEType, is_static: false, name: "setConversionMode" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getNewTextFormat, base_type: kTextFieldType, is_static: false, name: "getNewTextFormat" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_setNewTextFormat, base_type: kTextFieldType, is_static: false, name: "setNewTextFormat" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getDepth, base_type: kTextFieldType, is_static: false, name: "getDepth" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_swapDepths, base_type: kTextFieldType, is_static: false, name: "swapDepths" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getInstanceAtDepth, base_type: kTextFieldType, is_static: false, name: "getInstanceAtDepth" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getNextHighestDepth, base_type: kTextFieldType, is_static: false, name: "getNextHighestDepth" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_replaceSel, base_type: kTextFieldType, is_static: false, name: "replaceSel" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getLineIndexOfCharacter, base_type: kTextFieldType, is_static: false, name: "getLineIndexOfCharacter" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getSelectionBeginIndex, base_type: kTextFieldType, is_static: false, name: "getSelectionBeginIndex" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getSelectionEndIndex, base_type: kTextFieldType, is_static: false, name: "getSelectionEndIndex" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getCaretIndex, base_type: kTextFieldType, is_static: false, name: "getCaretIndex" },
    AscWarningInstance { code: kUnsupportedMeth_TextField_getFontList, base_type: kTextFieldType, is_static: false, name: "getFontList" },
    AscWarningInstance { code: kUnsupportedMeth_TextSnapshot_getCount, base_type: kTextSnapshotType, is_static: false, name: "getCount" },
    AscWarningInstance { code: kUnsupportedMeth_URLLoader_send, base_type: kURLLoaderType, is_static: false, name: "send" },
    AscWarningInstance { code: kUnsupportedMeth_URLStream_available, base_type: kURLStreamType, is_static: false, name: "available" },
    AscWarningInstance { code: kUnsupportedMeth_URLRequest_addRequestHeader, base_type: kURLRequestType, is_static: false, name: "addRequestHeader" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_load, base_type: kXMLDocumentType, is_static: false, name: "load" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_send, base_type: kXMLDocumentType, is_static: false, name: "send" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_sendAndLoad, base_type: kXMLDocumentType, is_static: false, name: "sendAndLoad" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_onData, base_type: kXMLDocumentType, is_static: false, name: "onData" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_addRequestHeader, base_type: kXMLDocumentType, is_static: false, name: "addRequestHeader" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_getBytesLoaded, base_type: kXMLDocumentType, is_static: false, name: "getBytesLoaded" },
    AscWarningInstance { code: kUnsupportedMeth_XMLDocument_getBytesTotal, base_type: kXMLDocumentType, is_static: false, name: "getBytesTotal" },
    AscWarningInstance { code: kUnsupportedMeth_Button, base_type: kVoidType, is_static: false, name: "Button" },
    AscWarningInstance { code: kUnsupportedMeth_Container, base_type: kVoidType, is_static: false, name: "Container" },
    AscWarningInstance { code: kUnsupportedMeth_Image, base_type: kVoidType, is_static: false, name: "Image" },
    AscWarningInstance { code: kUnsupportedMeth_ImageFilter, base_type: kVoidType, is_static: false, name: "ImageFilter" },
    AscWarningInstance { code: kUnsupportedMeth_ImageSprite, base_type: kVoidType, is_static: false, name: "ImageSprite" },
    AscWarningInstance { code: kUnsupportedMeth_ImageLoaderInfo, base_type: kVoidType, is_static: false, name: "ImageLoaderInfo" },
    AscWarningInstance { code: kUnsupportedMeth_ImeEvent, base_type: kVoidType, is_static: false, name: "ImeEvent" },
    AscWarningInstance { code: kUnsupportedMeth_Key, base_type: kVoidType, is_static: false, name: "Key" },
    AscWarningInstance { code: kUnsupportedMeth_LineMetrics, base_type: kVoidType, is_static: false, name: "LineMetrics" },
    AscWarningInstance { code: kUnsupportedMeth_LoadVars, base_type: kVoidType, is_static: false, name: "LoadVars" },
    AscWarningInstance { code: kUnsupportedMeth_MenuEvent, base_type: kVoidType, is_static: false, name: "MenuEvent" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClipLoader, base_type: kVoidType, is_static: false, name: "MovieClipLoader" },
    AscWarningInstance { code: kUnsupportedMeth_SystemCapabilities, base_type: kVoidType, is_static: false, name: "SystemCapabilities" },
    AscWarningInstance { code: kUnsupportedMeth_SystemCapabilities, base_type: kVoidType, is_static: false, name: "SystemCapabilities" },
    AscWarningInstance { code: kUnsupportedMeth_TextExtents, base_type: kVoidType, is_static: false, name: "TextExtents" },
    AscWarningInstance { code: kUnsupportedMeth_System_showSettings, base_type: kSystemType, is_static: true, name: "showSettings" },
    AscWarningInstance { code: kUnsupportedMeth_BitmapData_loadBitmap, base_type: kBitmapDataType, is_static: true, name: "loadBitmap" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_loadVariables, base_type: kMovieClipType, is_static: false, name: "loadVariables" },
    AscWarningInstance { code: kUnsupportedMeth_FileReferenceList_addListener, base_type: kFileReferenceListType, is_static: false, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_FileReferenceList_removeListener, base_type: kFileReferenceListType, is_static: false, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_Date_getYear, base_type: kDateType, is_static: false, name: "getYear" },
    AscWarningInstance { code: kUnsupportedMeth_Date_setYear, base_type: kDateType, is_static: false, name: "setYear" },
    AscWarningInstance { code: kUnsupportedMeth_Date_getUTCYear, base_type: kDateType, is_static: false, name: "getUTCYear" },
    AscWarningInstance { code: kUnsupportedMeth_Microphone_setGain, base_type: kMicrophoneType, is_static: false, name: "setGain" },
    AscWarningInstance { code: kUnsupportedMeth_Microphone_setRate, base_type: kMicrophoneType, is_static: false, name: "setRate" },
    AscWarningInstance { code: kUnsupportedMeth_Selection, base_type: kVoidType, is_static: false, name: "Selection" },
    AscWarningInstance { code: kUnsupportedMeth_ColorTransform_getTransform, base_type: kColorTransformType, is_static: false, name: "getTransform" },
    AscWarningInstance { code: kUnsupportedMeth_ColorTransform_setTransform, base_type: kColorTransformType, is_static: false, name: "setTransform" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_loadSound, base_type: kSoundType, is_static: false, name: "loadSound" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_getVolume, base_type: kSoundType, is_static: false, name: "getVolume" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_getTransform, base_type: kSoundType, is_static: false, name: "getTransform" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_getPan, base_type: kSoundType, is_static: false, name: "getPan" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_setPan, base_type: kSoundType, is_static: false, name: "setPan" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_beginBitmapFill, base_type: kDisplayObjectType, is_static: false, name: "beginBitmapFill" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_hitTest, base_type: kDisplayObjectType, is_static: false, name: "hitTest" },
    AscWarningInstance { code: kUnsupportedMeth_MovieClip_attachBitmap, base_type: kDisplayObjectType, is_static: false, name: "attachBitmap" },
    AscWarningInstance { code: kUnsupportedMeth_call, base_type: kVoidType, is_static: false, name: "call" },
    AscWarningInstance { code: kUnsupportedMeth_Color, base_type: kVoidType, is_static: false, name: "Color" },
    AscWarningInstance { code: kUnsupportedMeth_capabilities, base_type: kVoidType, is_static: false, name: "capabilities" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_getBytesLoaded, base_type: kSoundType, is_static: false, name: "getBytesLoaded" },
    AscWarningInstance { code: kUnsupportedMeth_Sound_getBytesTotal, base_type: kSoundType, is_static: false, name: "getBytesTotal" },
    AscWarningInstance { code: kUnsupportedMeth_TextFormat_getTextExtent, base_type: kTextFormatType, is_static: false, name: "getTextExtent" },
    AscWarningInstance { code: kUnsupportedMeth_FileReference_addListener, base_type: kFileReferenceType, is_static: false, name: "addListener" },
    AscWarningInstance { code: kUnsupportedMeth_FileReference_removeListener, base_type: kFileReferenceType, is_static: false, name: "removeListener" },
    AscWarningInstance { code: kUnsupportedMeth_eval, base_type: kVoidType, is_static: false, name: "eval" },
    AscWarningInstance { code: kUnsupportedMeth_getVersion, base_type: kVoidType, is_static: false, name: "getVersion" },
    AscWarningInstance { code: kUnsupportedMeth_ifFrameLoaded, base_type: kVoidType, is_static: false, name: "ifFrameLoaded" },
];

pub static unsupportedEvents: [AscWarningInstance; 81] = [
    AscWarningInstance { code: kWarning_Event_onStatus, base_type: kSystemType, is_static: false, name: "onStatus" },
    AscWarningInstance { code: kWarning_Event_onID3, base_type: kSoundType, is_static: false, name: "onID3" },
    AscWarningInstance { code: kWarning_Event_onLoad, base_type: kSoundType, is_static: false, name: "onLoad" },
    AscWarningInstance { code: kWarning_Event_onSoundComplete, base_type: kSoundType, is_static: false, name: "onSoundComplete" },
    AscWarningInstance { code: kWarning_Event_onResize, base_type: kStageType, is_static: false, name: "onResize" },
    AscWarningInstance { code: kWarning_Event_onChanged, base_type: kTextFieldType, is_static: false, name: "onChanged" },
    AscWarningInstance { code: kWarning_Event_onKillFocus, base_type: kTextFieldType, is_static: false, name: "onKillFocus" },
    AscWarningInstance { code: kWarning_Event_onScroller, base_type: kTextFieldType, is_static: false, name: "onScroller" },
    AscWarningInstance { code: kWarning_Event_onSetFocus, base_type: kTextFieldType, is_static: false, name: "onSetFocus" },
    AscWarningInstance { code: kWarning_Event_onLoad, base_type: kStyleSheetType, is_static: false, name: "onLoad" },
    AscWarningInstance { code: kWarning_Event_onMouseDown, base_type: kMouseType, is_static: false, name: "onMouseDown" },
    AscWarningInstance { code: kWarning_Event_onMouseUp, base_type: kMouseType, is_static: false, name: "onMouseUp" },
    AscWarningInstance { code: kWarning_Event_onMouseMove, base_type: kMouseType, is_static: false, name: "onMouseMove" },
    AscWarningInstance { code: kWarning_Event_onMouseWheel, base_type: kMouseType, is_static: false, name: "onMouseWheel" },
    AscWarningInstance { code: kWarning_Event_onKeyDown, base_type: kKeyboardType, is_static: false, name: "onKeyDown" },
    AscWarningInstance { code: kWarning_Event_onKeyUp, base_type: kKeyboardType, is_static: false, name: "onKeyUp" },
    AscWarningInstance { code: kWarning_Event_onData, base_type: kLoaderType, is_static: false, name: "onData" },
    AscWarningInstance { code: kWarning_Event_onLoad, base_type: kLoaderType, is_static: false, name: "onLoad" },
    AscWarningInstance { code: kWarning_Event_onDragOut, base_type: kSimpleButtonType, is_static: false, name: "onDragOut" },
    AscWarningInstance { code: kWarning_Event_onDragOver, base_type: kSimpleButtonType, is_static: false, name: "onDragOver" },
    AscWarningInstance { code: kWarning_Event_onKeyDown, base_type: kSimpleButtonType, is_static: false, name: "onKeyDown" },
    AscWarningInstance { code: kWarning_Event_onKeyUp, base_type: kSimpleButtonType, is_static: false, name: "onKeyUp" },
    AscWarningInstance { code: kWarning_Event_onKillFocus, base_type: kSimpleButtonType, is_static: false, name: "onKillFocus" },
    AscWarningInstance { code: kWarning_Event_onPress, base_type: kSimpleButtonType, is_static: false, name: "onPress" },
    AscWarningInstance { code: kWarning_Event_onRelease, base_type: kSimpleButtonType, is_static: false, name: "onRelease" },
    AscWarningInstance { code: kWarning_Event_onReleaseOutside, base_type: kSimpleButtonType, is_static: false, name: "onReleaseOutside" },
    AscWarningInstance { code: kWarning_Event_onRollOut, base_type: kSimpleButtonType, is_static: false, name: "onRollOut" },
    AscWarningInstance { code: kWarning_Event_onRollOver, base_type: kSimpleButtonType, is_static: false, name: "onRollOver" },
    AscWarningInstance { code: kWarning_Event_onSetFocus, base_type: kSimpleButtonType, is_static: false, name: "onSetFocus" },
    AscWarningInstance { code: kWarning_Event_onActivity, base_type: kCameraType, is_static: false, name: "onActivity" },
    AscWarningInstance { code: kWarning_Event_onStatus, base_type: kCameraType, is_static: false, name: "onStatus" },
    AscWarningInstance { code: kWarning_Event_onSelect, base_type: kContextMenuType, is_static: false, name: "onSelect" },
    AscWarningInstance { code: kWarning_Event_onSelect, base_type: kContextMenuItemType, is_static: false, name: "onSelect" },
    AscWarningInstance { code: kWarning_Event_onActivity, base_type: kMicrophoneType, is_static: false, name: "onActivity" },
    AscWarningInstance { code: kWarning_Event_onStatus, base_type: kMicrophoneType, is_static: false, name: "onStatus" },
    AscWarningInstance { code: kWarning_Event_onData, base_type: kDisplayObjectType, is_static: false, name: "onData" },
    AscWarningInstance { code: kWarning_Event_onDragOut, base_type: kDisplayObjectType, is_static: false, name: "onDragOut" },
    AscWarningInstance { code: kWarning_Event_onDragOver, base_type: kDisplayObjectType, is_static: false, name: "onDragOver" },
    AscWarningInstance { code: kWarning_Event_onEnterFrame, base_type: kDisplayObjectType, is_static: false, name: "onEnterFrame" },
    AscWarningInstance { code: kWarning_Event_onKeyDown, base_type: kDisplayObjectType, is_static: false, name: "onKeyDown" },
    AscWarningInstance { code: kWarning_Event_onKeyUp, base_type: kDisplayObjectType, is_static: false, name: "onKeyUp" },
    AscWarningInstance { code: kWarning_Event_onKillFocus, base_type: kDisplayObjectType, is_static: false, name: "onKillFocus" },
    AscWarningInstance { code: kWarning_Event_onLoad, base_type: kDisplayObjectType, is_static: false, name: "onLoad" },
    AscWarningInstance { code: kWarning_Event_onMouseDown, base_type: kDisplayObjectType, is_static: false, name: "onMouseDown" },
    AscWarningInstance { code: kWarning_Event_onMouseMove, base_type: kDisplayObjectType, is_static: false, name: "onMouseMove" },
    AscWarningInstance { code: kWarning_Event_onMouseUp, base_type: kDisplayObjectType, is_static: false, name: "onMouseUp" },
    AscWarningInstance { code: kWarning_Event_onPress, base_type: kDisplayObjectType, is_static: false, name: "onPress" },
    AscWarningInstance { code: kWarning_Event_onRelease, base_type: kDisplayObjectType, is_static: false, name: "onRelease" },
    AscWarningInstance { code: kWarning_Event_onReleaseOutside, base_type: kDisplayObjectType, is_static: false, name: "onReleaseOutside" },
    AscWarningInstance { code: kWarning_Event_onRollOut, base_type: kDisplayObjectType, is_static: false, name: "onRollOut" },
    AscWarningInstance { code: kWarning_Event_onRollOver, base_type: kDisplayObjectType, is_static: false, name: "onRollOver" },
    AscWarningInstance { code: kWarning_Event_onSetFocus, base_type: kDisplayObjectType, is_static: false, name: "onSetFocus" },
    AscWarningInstance { code: kWarning_Event_onUnload, base_type: kDisplayObjectType, is_static: false, name: "onUnload" },
    AscWarningInstance { code: kWarning_Event_onLoadComplete, base_type: kLoaderType, is_static: false, name: "onLoadComplete" },
    AscWarningInstance { code: kWarning_Event_onLoadError, base_type: kLoaderType, is_static: false, name: "onLoadError" },
    AscWarningInstance { code: kWarning_Event_onLoadInit, base_type: kLoaderType, is_static: false, name: "onLoadInit" },
    AscWarningInstance { code: kWarning_Event_onLoadProgress, base_type: kLoaderType, is_static: false, name: "onLoadProgress" },
    AscWarningInstance { code: kWarning_Event_onLoadStart, base_type: kLoaderType, is_static: false, name: "onLoadStart" },
    AscWarningInstance { code: kWarning_Event_onStatus, base_type: kNetStreamType, is_static: false, name: "onStatus" },
    AscWarningInstance { code: kWarning_Event_onData, base_type: kXMLType, is_static: false, name: "onData" },
    AscWarningInstance { code: kWarning_Event_onLoad, base_type: kXMLType, is_static: false, name: "onLoad" },
    AscWarningInstance { code: kWarning_Event_onClose, base_type: kXMLSocketType, is_static: false, name: "onClose" },
    AscWarningInstance { code: kWarning_Event_onConnect, base_type: kXMLSocketType, is_static: false, name: "onConnect" },
    AscWarningInstance { code: kWarning_Event_onData, base_type: kXMLSocketType, is_static: false, name: "onData" },
    AscWarningInstance { code: kWarning_Event_onXML, base_type: kXMLSocketType, is_static: false, name: "onXML" },
    AscWarningInstance { code: kWarning_Event_onSelect, base_type: kFileReferenceListType, is_static: false, name: "onSelect" },
    AscWarningInstance { code: kWarning_Event_onCancel, base_type: kFileReferenceListType, is_static: false, name: "onCancel" },
    AscWarningInstance { code: kWarning_Event_onData, base_type: kXMLDocumentType, is_static: false, name: "onData" },
    AscWarningInstance { code: kWarning_Event_onHTTPStatus, base_type: kXMLDocumentType, is_static: false, name: "onHTTPStatus" },
    AscWarningInstance { code: kWarning_Event_onLoad, base_type: kXMLDocumentType, is_static: false, name: "onLoad" },
    AscWarningInstance { code: kWarning_Event_onStatus, base_type: kSharedObjectType, is_static: false, name: "onStatus" },
    AscWarningInstance { code: kWarning_Event_onStatus, base_type: kLocalConnectionType, is_static: false, name: "onStatus" },
    AscWarningInstance { code: kWarning_Event_onIMEComposition, base_type: kIMEType, is_static: false, name: "onIMEComposition" },
    AscWarningInstance { code: kWarning_Event_onCancel, base_type: kFileReferenceType, is_static: false, name: "onCancel" },
    AscWarningInstance { code: kWarning_Event_onComplete, base_type: kFileReferenceType, is_static: false, name: "onComplete" },
    AscWarningInstance { code: kWarning_Event_onHTTPError, base_type: kFileReferenceType, is_static: false, name: "onHTTPError" },
    AscWarningInstance { code: kWarning_Event_onIOError, base_type: kFileReferenceType, is_static: false, name: "onIOError" },
    AscWarningInstance { code: kWarning_Event_onOpen, base_type: kFileReferenceType, is_static: false, name: "onOpen" },
    AscWarningInstance { code: kWarning_Event_onProgress, base_type: kFileReferenceType, is_static: false, name: "onProgress" },
    AscWarningInstance { code: kWarning_Event_onSecurityError, base_type: kFileReferenceType, is_static: false, name: "onSecurityError" },
    AscWarningInstance { code: kWarning_Event_onSelect, base_type: kFileReferenceType, is_static: false, name: "onSelect" },
];
