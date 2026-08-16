# AI_SYNC.md 鈥斺€?鍫垎闀夸箙鎬ц仈鍔ㄦ満鍒?
> **浠讳綍 AI 浠ｇ悊鎴栧紑鍙戣€呭湪鍫垎浠撳簱宸ヤ綔鍓嶏紝蹇呴』鍏堣瀹屾湰鏂囦欢骞剁櫥璁帮紱鏀跺伐蹇呴』鍥炶銆?*
> 鏈枃浠舵槸鎵€鏈夎凯浠ｈ€咃紙浜虹被涓?AI锛夌殑"浼氱绨?涓?浣滄垬鍦板浘"銆?> 閬靛惊 [agents.md](https://agents.md) 瑙勮寖锛涙湰鏂囦欢鏄粨搴撶骇 AGENTS.md 鐨勫己鍒跺墠缃槄璇汇€?
---

## 0. 鑱斿姩鍗忚锛堝己鍒讹級

### 0.1 寮€宸ュ墠锛圫tep 0锛岀己涓€涓嶅彲锛?
1. **璇绘湰鏂囦欢**锛毬? 鐘舵€佸揩鐓э紙宸插畬鎴?寰呭畬鎴愶級銆伮? 杩唬浼氱绨匡紙浠栦汉鍦ㄥ仛浠€涔堬級銆?2. **璇绘€昏**锛歚docs/MASTERPLAN.md` 绗叚閮ㄥ垎锛?8+ 椤硅鍐筹級涓?搂6.4 闃舵娓呭崟鈥斺€旇鍐冲嵆鍘嗗彶缁撹锛屼笉寰楁帹缈婚噸鏉ワ紝闄ら潪鏂板瑁佸喅鏉＄洰銆?3. **璇讳唬鐮佺湡鐩?*锛歚kanyu introspect --json`锛堟ā鍧?宸ュ叿/鏍煎紡鐭╅樀鐨勫崟涓€浜嬪疄鏉ユ簮锛夈€?4. **鐧昏寮€宸?*锛堣 搂0.2锛夛紝**鐒跺悗鎵嶅姩鎵?*銆?
### 0.2 寮€宸ョ櫥璁帮紙鍏堜簬浠ｇ爜鏀瑰姩锛?
鍦?搂2 浼氱绨?*椤堕儴**杩藉姞涓€鏉★紙鈮? 琛岋級锛?
```
### [寮€宸 2026-08-03 <杩唬鑰呮爣璇? 鈥?<涓€鍙ヨ瘽鎰忓浘>
- 鑼冨洿锛?棰勮瑙﹀姩鐨勬ā鍧?鏂囦欢>
- 渚濇嵁锛?鎬昏鏉＄洰 / Issue / 瑁佸喅缂栧彿>
- 棰勮锛?浣撻噺浼拌锛堝皬/涓?澶э級>
```

- **鑼冨洿閬胯**锛氬厛璇诲凡鏈?寮€宸?鏉＄洰锛屼笌鍏堕噸鍙犵殑鑼冨洿椤诲彟閫夋垨绛夊緟锛涘悗鐧昏鑰呰琛屻€?- 杩唬鑰呮爣璇嗗缓璁甫韬唤锛屽 `kimi-code(agent-3)`銆乣claude-code`銆乣codex`銆佷汉绫?GitHub ID銆?
### 0.3 鏀跺伐鍥炶锛堥殢鏈€缁堟彁浜わ級

鍚屼竴浣嶇疆杩藉姞锛堚墹8 琛岋級锛?
```
### [鏀跺伐] 2026-08-03 <杩唬鑰呮爣璇? 鈥?<涓€鍙ヨ瘽缁撴灉>
- 鎻愪氦锛?hash 鍒楄〃>锛涙祴璇曪細<鏁板瓧>锛涢獙璇侊細<fmt/clippy/鍐掔儫>
- 鍋忓樊锛?涓庡師鎰忓浘鐨勫樊寮傚強鍘熷洜锛涙棤鍒欏啓"鏃?>
- 鍚庣画锛?鏂颁骇鐢熺殑寰呭姙锛屽凡鍚屾鍐欏叆 搂1.2>
```

骞跺悓姝ワ細鈶?搂1.1/搂1.2 鐘舵€佸揩鐓э紱鈶?娑夊強鑳藉姏鍙樺寲鏃舵洿鏂?`crates/kanyu-core/src/introspect.rs`锛堝崟涓€浜嬪疄鏉ユ簮锛夛紱鈶?CHANGELOG.md銆?
### 0.4 鏂囦欢绾緥

- **鍙涓嶆敼**锛氫細绛剧翱鍘嗗彶鏉＄洰姘镐笉鍒犳敼锛堢籂閿欎互鏂版潯鐩敞鏄庯級锛涙柊鏉＄洰姘歌繙鍔犲湪浼氱绨?*椤堕儴**銆?- **鍏堟媺鍚庢帹**锛氭敼鍔ㄦ湰鏂囦欢鍓?`git pull --rebase`锛屽啿绐佹椂淇濈暀鍙屾柟鏉＄洰锛堟寜鏃堕棿鎺掑簭锛夈€?- **鍗曞叆鍙?*锛氬崗璁彧鍐欏湪鏈枃浠讹紝AGENTS.md 涓?CONTRIBUTING.md 鎸囧悜杩欓噷锛屼笉澶嶅埗鏉℃銆?
---

## 1. 鐘舵€佸揩鐓?
> 姣忔鏀跺伐鍥炶鏃舵洿鏂般€傛埅鑷?**2026-08-13 路 v0.22.0+ 路 367 娴嬭瘯鍏ㄧ豢**锛坄agents validate` 鍙岃澧冨绾︽敹鍙ｏ細`to_ascii_lowercase` 瑙ｆ瀽淇 + `AGENTS.md` 闆跺弬/`--code-repo` 鍧囬€氳繃锛夈€?
### 1.1 宸插畬鎴愬疄鐜?
| 妯″潡 | 鐘舵€?| 鍐呭 |
|------|------|------|
| kanyu-core | 鉁?stable | GeoArrow RecordBatch 鍐呭瓨妯″瀷锛?7 鏍煎紡娉ㄥ唽琛紱AGENTS.md 璇箟灞傦紱绯荤粺鑷渷 |
| 鏍煎紡 I/O | 鉁?| GeoJSON/CSV/TSV/xlsx/SHP(璇诲啓)/FGB/GeoParquet/DXF/KML/KMZ/DWG(璇? 鍏ㄥ厤 GDAL |
| DWG | 鉁?璇? | acadrust+鑷寔琛ヤ竵灞傦紙瑁佸喅 #18锛夛紱鍏被鍑犱綍+鏍囨敞瑕佺礌+妞渾锛?43 鏍锋湰/52 涓囧疄浣撻獙璇?|
| 鍒嗘瀽 | 鉁?| buffer/overlay/topology/sjoin/zonal_stats/measure + EPSG 鍏ㄥ簱鎶曞奖锛沢eoprocess 涓夋壒 QGIS 绉绘锛堜竴鎵癸細dissolve/simplify/centroid/convex_hull/delete_holes/explode/stats锛涗簩鎵癸細boundary/bounding_boxes/merge/extract_by_attribute/extract_by_location/count_points_in_polygon/field_stats/mean_coordinates锛涗笁鎵癸細distance_matrix/nearest_neighbor/multi_ring_buffer/variable_buffer/split_by_field/add_geometry_attributes/create_grid/points_along_lines/concave_hull/minimum_rotated_rect锛?|
| kanyu-render | 鉁?| 绂诲睆 PNG/SVG锛涙櫒灞?澶滆鏄燂紱graduated/categorical 绗﹀彿鍖?|
| kanyu-mcp | 鉁?| 17 stable 宸ュ叿锛泂tdio+streamable HTTP锛汼EP-2663 闀夸换鍔?|
| kanyu-skill | 馃毀 incubating | wasmtime+WIT 瀹夸富锛涚噧鏂欐矙绠憋紱MCP 鐑姞杞斤紙hotload/skill_run/skill_list锛?|
| kanyu-cli | 鉁?| 7 鍛戒护缁勶紝鍏ㄥ眬 --json锛泇0.14.0 宸插彂甯冨苟瀹夎 |
| kanyu-shell | 馃毀 incubating | v0.8锛氬懡浠ゆ敞鍐岃〃锛圖AML 鎶曞奖锛夈€乨ock 涓夊尯鍋滈潬+婊氬姩濂戠害銆佷腑澶鍥惧仠闈犲尯锛堝湴鍥炬椤电鍚搁檮/绾櫧鐢诲竷锛夈€乻ymbology 绗﹀彿鍖栵紙鍗曡壊/鍞竴鍊?鍒嗙骇锛屾寜灞傛覆鏌撳叆 .kyu锛夈€乧atalog 宸ョ▼鐩綍浜斿垎绫汇€乼oolbox 鍙傛暟绫诲瀷瀵归綈 ArcGIS Python 宸ュ叿绠辫鑼冿紙杩涘害妯℃€佸彲鍙栨秷/涓夌骇鏃ュ織锛夈€佸睘鎬ц〃+瀛楁璁＄畻鍣ㄣ€佸瑙嗗浘+瀹為獙 3D銆乄CAG 2.2/鐘舵€佽壊浣撶郴 |
| kanyu-py | 鉁?| 48 缁戝畾锛坓eoprocess 涓夋壒/attrcalc/crs 妫€绱?toolrun锛? Layer 28 閾惧紡鏂规硶 + toolbox registry |
| 鍫垎鏁版嵁搴?.kdb | 鉁?| 鑷爺瀛樻。锛堣鍐?#19锛夛細Arrow IPC + kanyu.* 鍏冩暟鎹紝RecordBatch 鐩撮€氱被鍨嬩繚鐪燂紝鍏ㄦ牸寮忚浆鎹㈡帴鍏?|
| 鍫垎宸ョ▼ .kyu | 鉁?| JSON 宸ョ▼娓呭崟锛堣鍐?#19锛夛細鍥惧眰寮曠敤/瑙嗗彛/鍦板浘鑹插僵/鍙鎬э紝澹冲眰鎵撳紑/淇濆瓨 |
| 寮€婧愯鑼?| 鉁?| 鍙岃鍙?CI/Release 宸ヤ綔娴?浜斾唤鎺ュ彛鏂囨。/README 瀹炴媿鍥?|
| 涓婃父鍥為 | 鉁?| acadrust issue #55锛圓C15 瀹氫綅缂洪櫡 + 淇硶 + 璇佹嵁锛?|

### 1.2 寰呭畬鎴愪簨椤癸紙浼樺厛绾у簭锛?
1. **鍩虹 GIS 鍔熻兘绉绘**锛堢敤鎴锋寚浠わ紝杩涜涓級锛氬畻鍦?TXT + 鍥惧眰缁熻锛堢涓€鎵癸級銆乬eoprocess 绗簩鎵?8 绠楁硶锛坴0.16.0锛変笌绗笁鎵?10 绠楁硶锛坴0.17.0锛夊凡钀藉湴锛涘３灞?QGIS 寮忓伐鍏风 37 宸ュ叿鍙敤锛涘悗缁壒娆¤ 搂6.4 绉绘娓呭崟涓?ARCHITECTURE 搂9.1 璺嚎鎺ㄨ崘
2. **crates.io 鍙戝竷**锛氬叚涓悕绉板彲娉ㄥ唽锛屽緟鐢ㄦ埛 cargo login锛堝彂甯冮『搴?core鈫抮ender鈫抯kill鈫抦cp鈫抍li锛?3. **DWG 娣卞寲**锛堢敤鎴峰喅瀹氬悗缃級锛欼NSERT 鎷嗗潡 / HATCH 杈圭晫 / SPLINE 閲囨牱 / R2018+ 澶嶆祴
4. **Phase 2 瑙嗙晫缁?*锛歸gpu 瀹炴椂娓叉煋绠＄嚎锛圞anyuDB鈫扴SBO锛夈€丮LT 鐡︾墖銆丼DF 鏂囧瓧
5. **Phase 3 鎵?*锛欴CEL 澧為噺鎷撴墤缂栬緫鍐呮牳銆乁ndo/Redo
6. **Phase 4 鑴?*锛歀LM 铻嶅悎锛堣嚜鐒惰瑷€鈫掑伐鍏疯皟鐢ㄧ紪鎺掞級銆丮CP resources/prompts銆丟eoAnalystBench 鍩哄噯
7. **Phase 5 榄傜画**锛氭妧鑳藉競鍦恒€丄/B 娴嬭瘯妗嗘灦銆佺煡璇嗗簱 RAG
8. **鎬ц兘鍩哄噯**锛氬 QGIS 鐨?搂5.3 鎸囨爣瀹炴祴骞跺叕寮€鍩哄噯鎶ュ憡
9. **parquet codec 瑁佸壀**锛歾std-sys 绛?C codec 缁?parquet 寮曞叆锛岃瘎浼拌鍓繚鎸?鍐呮牳闆?C"绾害
10. **灞炴€ч潰鏉块噸寤?*锛氱瓑寰呯敤鎴峰畾鍒惰姹?
### 1.3 鑷垜杩唬杈圭晫锛堜笉鍙€捐秺锛?
- **鍫垎鐏典笉鍦ㄧ敤鎴疯繍琛屾椂鐩存帴淇敼鍐呮牳**銆傝嚜鎴戣凯浠ｅ彂鐢熷湪 **GitHub 鍗忎綔灞?*锛?  鎵€鏈夊彉鏇寸粡鎻愪氦/PR 杩涘叆浠撳簱锛孋I锛坒mt+clippy+test+deny锛夊繀椤诲叏缁匡紝
  鍐呮牳鍚堝苟椤讳汉閬撴槑杩滃鏍革紙鐜伴樁娈碉級锛沇ASM 鎶€鑳界儹鍔犺浇鏄敮涓€鍏嶅鏍搁€氶亾
  锛堟矙绠遍殧绂伙紝涓嶆敼鍐呮牳锛夈€?- 浠讳綍 AI 涓嶅緱鍒犻櫎/寮卞寲鏈竟鐣屾潯娆撅紱淇鍙兘浠ユ柊瑁佸喅鏉＄洰杩藉姞杩涙€昏 搂6.1銆?
---

## 2. 杩唬浼氱绨匡紙鏂版潯鐩姞鍦ㄩ《閮級

### [鏀跺伐] 2026-08-13 dsh-qwen(main) 鈥?`agents validate` 鏍￠獙濂戠害鏀跺彛锛坄to_ascii_lowercase` 瑙ｆ瀽淇锛?- 鎻愪氦锛?hash 瑙佹湰娆℃彁浜?锛涙祴璇曪細367 鍏ㄧ豢 + clippy 闆惰鍛?+ fmt 鍑€锛坄to_ascii_lowercase` 绾В鏋愬眰鏀瑰姩锛屼笉瑙﹀強鏍￠獙鍒ゅ畾锛屾祴璇曢潰鍥炲綊闆跺彉鍖栵級
- 楠岃瘉锛歝lean `release` 閲嶅缓 6m10s锛坄BUILD_EXIT=0`锛夆啋 `kanyu agents validate --path AGENTS.md` 闆跺弬閫氳繃锛? 鍥惧眰/0 瑙勫垯锛夈€乣--code-repo` 閫氳繃鈥斺€旀牴鍥犲畾浣嶏細`to_ascii_lowercase` 鍒嗘敮涓?`data-layer: 鍚 琛岋紙閿悗鏃?`**bold**` 宓屽锛夋紡璧?`trim_start(boundary)` 瑙勮寖鍖栵紝`split_once("**: ")` 鎵句笉鍒?`"data-layer: "` 鑷?`data_layer` 璇垽 None 鈫?璇寜 crs 鍗犱綅銆屼笉閫傜敤銆嶈蛋浠ｇ爜搴撳厤妫€鍒嗘敮锛屽嵈鎶婄己澶辩殑 `name`/`crs` 璁颁负杞憡璀︼紝涓?0 鍥惧眰/0 瑙勫垯鑷浉鐭涚浘锛涗慨澶嶅悗闆跺弬鍗宠蛋銆屾樉寮?`data-layer: 鍚 鈫?鍏嶆銆嶆潈濞佽矾寰?- 鍋忓樊锛歚AI_SYNC.md` 鏈韩缁?`validate --path AI_SYNC.md` 浠嶆姤 2 闂锛屽潎涓恒€岃蒋浠朵粨搴擄細寤鸿琛ュ叏/寤鸿鍐欎笉閫傜敤銆?*杞憡璀?*锛坄name`/`crs` 瀵圭函杞粨搴撴湰鍗抽潪蹇呭～锛屽睘鏍￠獙鍣ㄥ鍚屾绫绘枃浠剁殑鎺緸杩囦弗锛屼笉闃绘柇锛夛紱鏈粨搴?`AGENTS.md`锛堟牎楠岀湡姝ｇ殑鐩爣鏂囦欢锛変笁鎬佸叏杩?- 鍚庣画锛歚validate` 瀵硅蒋浠撳簱寤鸿椤规槸鍚﹀簲闄嶄负鎻愮ず绾т笉璁℃暟锛堢嫭绔嬪皬鏀癸紝鏈贩鍏ユ湰娆′慨澶嶄互淇濇寔鍗曚竴鑱岃矗锛夛紱`build.log`/`test.log`/`clippy.log`/`build_clean.log` 涓存椂杈撳嚭鎸?搂5 鏂囦欢绾緥寰呭綊妗ｆ垨鍒?
### [寮€宸 2026-08-13 dsh-qwen(main) 鈥?鍫垎GIS 脳 DeepSeek Harness 缁勪欢锛堜竷澶ц兘鍔涚Щ妞嶏級+ GIS妯″紡
- 鑼冨洿锛歞sh/ 鏂扮洰褰曪紙kanyu-gis 缁勪欢鍙屽崐浠ｇ爜 + kanyu-gis 妯″紡 preset 妯℃澘 + README + 绀轰緥鏁版嵁锛夈€丄I_SYNC/README/CHANGELOG
- 渚濇嵁锛氱敤鎴锋寚浠わ紙鍩轰簬鍫垎宸ョ▼鍒涘缓 DSH 缁勪欢锛氬湴鍥?鏁版嵁/鍧愭爣妗嗘灦/鐩綍/鍦扮悊澶勭悊/缂栬緫/3D 涓冨ぇ鑳藉姏绉绘杩涚粍浠惰嚜鎴戣凯浠ｏ紱寮€婧愬悓姝?GitHub锛涘熀浜庣粍浠跺垱寤?GIS 妯″紡闀挎湡鎺ㄨ繘锛?- 棰勮锛氬ぇ锛堢粍浠?Host+Client銆丟IS 妯″紡鏋勬垚銆佸紑婧愭帹閫併€佹湰鏈烘椿浣撻獙璇侊級

### [鏀跺伐] 2026-08-12 kimi-code(main) 鈥?AI 鎰忓浘璇勪及闆嗗熀鍑嗭紙Phase 4 娓呭崟鏀跺畼锛?- 鎻愪氦锛氳鏈 commit锛涙祴璇曪細367 鍏ㄧ豢 + clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛欵VAL_SET 40 鐢ㄤ緥 100% 閫氳繃锛堥槇鍊?90% 瀹堟姢锛夛紱璇勪及椹卞姩 4 澶勮В鏋愪慨姝ｏ紙浼樺厛绾у姭鎸?琛ㄨ揪寮忓悎骞?璇嶇紑鍖归厤/Crs 缂虹渷锛?- 鍚庣画锛堝潎闇€澶栭儴鏉′欢鎴栧鍛ㄥ伐绋嬶級锛氱湡瀹炵鐐硅仈璋冿紙寰?API key锛夈€乧rates.io 鍙戝竷锛堝緟 cargo login token锛夈€丟eoArrow 鍘熺敓鍒楄縼绉汇€丳hase 5 鎶€鑳藉競鍦?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?v0.22.0 鎹嗙増鍙戝竷
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?66 鍏ㄧ豢 + clippy 闆惰鍛?+ fmt 鍑€锛汳SI 鍗囩骇瀹夎楠岃瘉
- 鍐呭锛氭崋鐗堝惈鈥斺€斿湴鍥炬缁戝畾鍥惧眰/鍏抽棴鈮犲垹闄?鍒嗗缓锛涚紪杈戜綋绯伙紙绾块潰缁樺埗/鎹曟崏/鎸栨礊/鎷撴墤鑱斿姩/鍒嗗壊瑕佺礌锛夛紱DCEL v1+v2锛泈gpu 3D 绠＄嚎涓ゆ壒锛汳CP resources/prompts锛汥elta 蹇収+浜嬪姟锛汚I 鎰忓浘闈?+ OpenAI function calling锛涘竷灞€ v2/鏈嶅姟閾炬帴 v2锛況star 瑁佸壀
- 鍚庣画锛欸eoAnalystBench銆佺湡瀹炵鐐硅仈璋冦€乧rates.io锛堝緟 token锛?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?鍒嗗壊瑕佺礌缂栬緫宸ュ叿
- 鎻愪氦锛氳鏈 commit锛涙祴璇曪細366 鍏ㄧ豢锛坋dit 38锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛歴plit.rs 涓ゆ搷浣滐紙蔚 缂撳啿宸泦鍔″疄璺嚎锛岀灞戦槇鍊煎叆妗ｏ級+ 澹冲眰鍒嗗壊宸ュ叿鎵嬪娍锛況ibbon 琛ユ寕 edit_topo 婕忚
- 鍚庣画锛欸eoAnalystBench銆佺湡瀹炵鐐硅仈璋冦€丏CEL 缂栬緫鑱斿姩娣卞寲

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?OpenAiDriver function calling
- 鎻愪氦锛?1526e6锛涙祴璇曪細366 鍏ㄧ豢锛坰hell 132锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛歵ools Schema 鎶曞奖/args 鎶樼畻/鈮? 杞皟鐢ㄥ惊鐜?杩囩▼琛岋紱绂荤嚎鍋囨ā鍨嬫祴璇?- 鍚庣画锛氬垎鍓茶绱犲伐鍏凤紙鍦ㄥ埗锛夈€丟eoAnalystBench銆佺湡瀹炵鐐硅仈璋?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?AI 瀵硅瘽鎰忓浘闈㈡帴宸ュ叿绠憋紙閰嶉宸叉仮澶嶏級
- 鎻愪氦锛歜9c4cdd锛涙祴璇曪細357 鍏ㄧ豢锛坰hell 127锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛歀ocalDriver 涓ょ骇鍖归厤/鍙傛暟绫诲瀷鍖栨彁鍙?缂哄弬寮曞锛沨ost_run_tool 澶嶇敤鍚庡彴閾捐矾锛涘府鍔╂姇褰?38 宸ュ叿锛泈gpu 娈嬮」琛ラ綈
- 鍚庣画锛歅hase 4 LLM function calling銆丏CEL 缂栬緫鑱斿姩娣卞寲

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?鎷撴墤缂栬緫鎺ョ嚎 + wgpu 绗簩鎵癸紙瀛愪唬鐞嗛厤棰濅腑鏂紝涓荤嚎绋嬫敹灏撅級
- 鎻愪氦锛?815c1f/ce81473锛涙祴璇曪細353 鍏ㄧ豢锛坰hell 123/edit 34锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍋忓樊锛氬瓙浠ｇ悊涓€?403锛堥厤棰濆懆鏈熻€楀敖锛夛紝topoedit 鍐呮牳涓庢祴璇曠敱鍏跺畬鎴愩€佸３灞傛帴绾匡紙寮€鍏?Delta 閫氶亾/鐘舵€佹爮锛夌敱涓荤嚎绋嬭ˉ榻愶紱wgpu 绗簩鎵瑰崐閫旂姸鎬佺敱涓荤嚎绋嬬画瀹岋紙娲炲唴澹?鑳屽墧/鍙岀粫鍚戞祴璇曟湡鏈涘€兼洿鏂般€佹埅鍥剧洰妫€锛?- 鍚庣画锛歅hase 4 LLM 铻嶅悎娣卞寲銆丏CEL 涓庣紪杈戣仈鍔ㄦ繁鍖栵紱瀛愪唬鐞嗛厤棰濆緟鍛ㄦ湡鍒锋柊

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?DCEL v2 + wgpu 绠＄嚎鍖栫涓€鎵?- 鎻愪氦锛?9f5275/1b6c446锛涙祴璇曪細348 鍏ㄧ豢锛坋dit 29/shell 119锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛歴tub 鐜璧拌浆鍚戣鍒欙紙鈮? 搴﹂《鐐瑰鎷嶄慨姝ｏ級+merge_faces 澧撶寮忥紱wgpu 椤剁偣缂撳瓨/绾跨偣缁樺埗/鑰冲垏鍚礊锛堜笁澶勭湡闂淇鍏ユ敞锛?澶氳鍙ｅ垎閿?- 鍚庣画锛歸gpu 绗簩鎵癸紙娲炲唴澹?鑳屽墧锛夈€丏CEL 鎺ョ紪杈戞搷浣溿€丳hase 4 LLM 铻嶅悎

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?DCEL v1 + wgpu 3D spike + 缂栬緫澧炲己锛堟崟鎹?鎸栨礊锛?- 鎻愪氦锛歞60d896/615303e/ea9e015锛涙祴璇曪細340 鍏ㄧ豢锛坋dit 24/shell 117锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛欴CEL 涓夎〃+瀛旀礊铏氶潰绾﹀畾+瀵硅绾垮垎瑁傦紙娆ф媺淇濇寔 6 鎷撴墤鏂█锛夛紱wgpu PaintCallback 绂诲睆鐪熸繁搴︽覆鏌撴１鏌憋紙杞欢鍥為€€淇濈暀锛夛紱椤剁偣鎹曟崏/闈㈡寲娲烇紙Intersects 杈圭晫淇锛?- 鍚庣画锛欴CEL v2锛堢粫澶栭潰閬嶅巻/merge_faces锛夈€亀gpu 姝ｅ紡绠＄嚎锛堢紦瀛?澶氳鍙?鑰冲垏娲炵幆锛夈€丳hase 4 LLM 铻嶅悎

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?MCP resources/prompts + 蹇嵎閿?楂樹寒/WMS 鍕鹃€夋寔涔呭寲
- 鎻愪氦锛?c85297/c1c8d10锛涙祴璇曪細332 鍏ㄧ豢锛坢cp 11/shell 117锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛歬anyu://formats/tools/crs/{code} 璧勬簮 + 涓変腑鏂?prompts锛圥ROMPTS 娉ㄥ唽琛級锛汣trl+Z/Y/S 蹇嵎閿紙text_edit_focused 瀹堝崼锛夛紱閫変腑瑕佺礌楂樹寒锛沇MS 鍕鹃€夊叆 ui-state锛堝伐绋嬩紭鍏堣涔夋敞鏄庯級
- 鍚庣画锛欴CEL銆?D 鐪熺绾?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?kanyu-edit v2 Delta 蹇収/浜嬪姟 + WMS 鍏?.kyu
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?28 鍏ㄧ豢锛坋dit 18锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛歞elta.rs锛堜笁鎬佺粺涓€/闃堝€?256/GeoArrow 璺嚎鐣欐。锛? 浜嬪姟鍘熷瓙鎻愪氦锛汸rojectFrame.wms_base 鎸佷箙鍖栵紙杩炴帴灞炴湰鏈哄彇鑸嶆敞鏄庯級+ 鏈嶅姟杩炴帴缂栬緫鍥炲～
- 鍚庣画锛欴CEL銆?D 鐪熺绾裤€丮CP resources/prompts

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?v0.21.0锛氬竷灞€ v2 + 鏈嶅姟閾炬帴 v2 + 绾块潰缁樺埗 + 鍦板浘妗嗘繁鍖?rstar 鎹嗙増
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?19 鍏ㄧ豢锛坰hell 115/render 23/core 138/edit 10锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛氣懇 甯冨眬 v2锛坒ontdue 绯荤粺 CJK 瀛椾綋鏍堚€斺€旇繍琛屾椂鍔犺浇涓嶅叆搴擄紝鍥為€€鐐归樀锛涘竷灞€鍏?.kyu 鍚戝悗鍏煎锛涘竷灞€缁戝畾鍦板浘妗嗕笉闅忔縺娲诲垏鎹級锛涒應 鏈嶅姟閾炬帴 v2锛圵FS GetCapabilities 鍥惧眰鍙戠幇鈥斺€旀墜鍐欐渶灏?XML 鎻愬彇锛沇MS 搴曞浘鍙犲姞鈥斺€旀寜瑙嗗彛 GetMap/缂撳瓨鍘绘姈/妗嗙骇缁戝畾/澶辫触涓嶉樆鏂級锛涚嚎闈㈢粯鍒讹紙缁樺埗鐘舵€佹満+姗＄毊绛?绫诲瀷闂ㄧ锛夛紱鈶?缂栬緫澧炲己锛堥《鐐规崟鎹?10px 鍙紑鍏?+ 闈㈡寲娲?AddHole 鍏?History + Multi* 閮ㄤ欢绾ф牳鏌ラ攣瀹氾紝kanyu-edit 10 娴嬶級锛涘惈 鈶р懆 鍦板浘妗嗘繁鍖栦笌 rstar 鎹嗙増
- 楠岃瘉锛氬竷灞€ PNG 涓枃鏍囬/鍥句緥钀界洏鍥剧洰妫€锛涚粦瀹氭鍒囨崲瀹炶瘉鎴浘锛涙湇鍔″彂鐜板璇濇鎴浘锛涚粯鍒舵鐨瓔鎴浘
- 鍋忓樊锛氭棤锛圵MS 涓ユ牸 1.3.0 杞村簭鏈嶅姟鍣ㄤ负宸茬煡杈圭晫锛屾敞閲婃敞鏄庯級
- 鍚庣画锛毬?.1 浣欌€斺€旂紪杈?Delta 蹇収/DCEL銆?D 鐪熺绾裤€乄MS 搴曞浘鐘舵€佸叆 .kyu

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?鍦板浘妗嗙粦瀹氬浘灞?+ 鍏抽棴鈮犲垹闄?+ 浜岀淮/涓夌淮鍒嗗缓 + rstar 瑁佸壀
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?08 鍏ㄧ豢锛坈ore 138/shell 109锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛氣懅 MapFrame 浜ゆ崲妯″瀷锛坧ark/unpark 鐜板満鍐荤粨锛屽垏鎹㈡鍥惧眰璺熼殢锛涘睘鎬ц〃/绗﹀彿鍖?缂栬緫/宸ュ叿绠卞叏鎸囧悜婵€娲绘锛夛紱鍏抽棴鈮犲垹闄わ紙鐩綍寮辫壊琛屽彲閲嶅紑锛屽彸閿垹闄わ級锛涗簩缁?涓夌淮鍒嗗缓锛涢粦杈逛笁澶勪慨澶嶏紱.kyu 鍔?map/frames锛堝悜鍚庡吋瀹癸級锛涒懆 rstar 绱㈠紩瑁佸壀 overlay/sjoin锛堝鎷嶉攣瀹氶泦鍚堢浉绛夛紱澶嶆祴 overlay 1.5x銆乻join 澶?join 渚?9.1x锛屄?.2 鍏ユ。锛?- 鍋忓樊锛氭棤
- 鍚庣画锛毬?.1 浣欌€斺€斿竷灞€ v2锛圥NG 涓枃/鍏?.kyu锛夈€佹湇鍔￠摼鎺?v2锛圙etCapabilities/WMS锛夈€佺紪杈?Delta 蹇収/DCEL銆?D 鐪熺绾?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?v0.20.0锛氬３灞傜紪杈戞ā寮?+ 鏈嶅姟閾炬帴 + 闀挎湡椤规崋鐗?- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?98 鍏ㄧ豢锛坰hell 103/edit 8/mcp 9/render 21/core 136 绛夛級+ clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛氣懃 澹冲眰缂栬緫妯″紡锛坋dit.rs 浼氳瘽 + 鐢诲竷椤剁偣鍙ユ焺鎷栨嫿/绉诲姩/鎻掔偣/鍒犻€?+ 灞炴€ц〃鍗曞厓鏍肩紪杈?+ 銆岀紪杈戙€嶅姛鑳藉尯椤电涓?QAT 鎾ら攢/閲嶅仛鎺ョ嚎锛屼繚瀛?鏀惧純浼氳瘽璇箟锛夛紱鈶?鏈嶅姟閾炬帴 v1锛坰ervices.rs WFS GetFeature锛氳繛鎺ョ鐞嗗叆 ui-state銆佸悗鍙扮嚎绋?杩涘害妯℃€佸彲鍙栨秷銆丟eoJSON 瑙ｆ瀽鐧昏鍥惧眰锛岀洰褰曚簲鍒嗙被鍏ㄩ儴鍏戠幇锛夛紱鎹嗙増 v0.20.0锛堝惈鍓嶅洓浠堕暱鏈熼」锛歬anyu-edit/甯冨眬/鍩哄噯/MCP 鏀舵暃/鐘舵€佹寔涔呭寲锛?- 楠岃瘉锛氱紪杈戞€佸彞鏌勬埅鍥俱€佹湇鍔￠摼鎺ュ璇濇涓庡垎绫昏鎴浘銆佸竷灞€椤电鎴浘鍧囩洰妫€閫氳繃锛沇FS 缃戠粶璺緞鏃犵绾挎祴璇曟湇鍔″櫒鏈疄鎵撳缃戯紙瑙ｆ瀽/鏍￠獙 4 娴嬭瘯绂荤嚎瑕嗙洊锛寀req API 绛惧悕鏍告簮鐮侊級
- 鍋忓樊锛氭棤
- 鍚庣画锛毬?.1 v0.20.0 浜旀潯锛堢紪杈戞繁鍖栫嚎闈㈡坊鍔?Delta 蹇収/DCEL锛涙€ц兘 rstar 涓庝簩杩涘埗瀵圭収锛涙湇鍔￠摼鎺?v2 GetCapabilities/WMS锛涘竷灞€ v2 涓枃瀛椾綋鏍?鍏?.kyu锛?D 鐪熺绾匡級

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?闀挎湡椤瑰洓浠讹細kanyu-edit 澧為噺 + 鎵撳嵃甯冨眬 + 鎬ц兘鍩哄噯 + 锛堝墠涓ゆ潯宸蹭細绛撅級
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?98 鍏ㄧ豢锛?72鈫?98锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛氣憿 kanyu-edit 鏂?crate锛圲ndo/Redo 妗嗘灦 + 浜斿熀纭€缂栬緫鍛戒护 + GeomPath 瀹氫綅锛? 鍗曟祴锛沬ntrospect/ARCHITECTURE 鐧昏锛夛紱鈶?render::layout 鎵撳嵃甯冨眬锛圓4 鎺掔増锛氭爣棰?鍦板浘/鍥句緥/姣斾緥灏?鎸囧寳閽堬紝SVG 瀹屾暣鏂囧瓧锛涘３灞傚竷灞€椤电涓庣洰褰曘€屽竷灞€妗嗐€嶅厬鐜帮紝瀵煎嚭 PNG/SVG锛? canvas composite_layers_png 鎸夊眰鍚堟垚锛涒懁 鎬ц兘鍩哄噯锛坈ore::bench 纭畾鎬у満鏅?+ kanyu analysis bench 浜旈」涓夋。锛岄杞疄娴嬪叆 搂8.1锛?00 涓囨。鍔犺浇 4.5s/buffer 9.3s/overlay 3.3s/sjoin 1.8s/render 3.8s锛孯yzen 9 9950X锛沷verlay 骞虫柟椤瑰潗瀹?rstar 璺嚎锛?- 楠岃瘉锛氬竷灞€椤电鎴浘鐩锛堟爣棰?鍥句緥/姣斾緥灏?鎸囧寳閽堥綈鍏級锛沚ench 涓夋。瀹炶窇
- 鍚庣画锛毬?.1 浣欌€斺€斿３灞傜紪杈戞ā寮忥紙kanyu-edit 鎺ョ嚎锛夈€佹湇鍔￠摼鎺ワ紙WFS/WMS锛夈€?D 鐪熺绾?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?闀挎湡椤逛袱浠讹細MCP 宸ュ叿闈㈡敹鏁?+ UI 鐘舵€佹寔涔呭寲
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?72 鍏ㄧ豢锛坢cp 6鈫?銆乻hell 90鈫?4锛? clippy 闆惰鍛?+ fmt 鍑€
- 鍐呭锛氣憼 MCP 鏂板 kanyu_toolbox_list/toolbox_run锛坱ooldef 娉ㄥ唽琛ㄦ姇褰?+ toolrun 缁熶竴鎵ц + SEP-2663 鐧藉悕鍗曠 7 甯級锛宨ntrospect 鐧昏锛孧CP.md 搂3.19/3.20鈥斺€斾笁闈竴澶勫０鏄庢敹鏁涜惤鍦帮紱鈶?uistate.rs锛氬仠闈?鏀惰棌/鏈€杩?缂╂斁/鍦板浘鑹插僵/宸ョ▼鍧愭爣绯?瑙嗗浘娓呭崟钀界洏 %LOCALAPPDATA%\kanyu\ui-state.json锛?s 闃叉姈+on_exit 鍐欑洏锛屽潖鏂囦欢 .bad 澶囦唤鍥為€€锛夛紝涓よ疆鍚姩鎴浘瀹炶瘉鎭㈠閾捐矾
- 鍋忓樊锛氭棤锛堢洰褰曞睍寮€鐘舵€佸彇鑸嶄负涓嶅瓨锛屾敞閲婃敞鏄庯級
- 鍚庣画锛毬?.1 浣欎笅鈥斺€旀€ц兘鍩哄噯瀹炴祴銆佺紪杈戝唴鏍搞€佸竷灞€妗?鏈嶅姟閾炬帴

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?v0.19.0锛氶潰鏉挎粴鍔ㄥ姞鍥?+ 鍦板浘妗嗗惛闄?+ 绗﹀彿鍖?+ 鐩綍鍒嗙被 + 鍙傛暟绫诲瀷瑙勮寖 + 閰嶈壊浣撶郴
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?65 鍏ㄧ豢锛?52鈫?65 鍏樁娈电疮璁★級+ clippy 闆惰鍛?+ fmt 鍑€
- 楠岃瘉锛氭埅鍥鹃泦鐩锛?5 鍥惧眰/5000 瑕佺礌婊氬姩鍘嬪姏銆佸惛闄勯〉绛?娴姩绐楀悓妗嗐€佺函鐧界敾甯冨弻涓婚銆佺鍙峰寲灞曞紑涓庡睘鎬ч〉銆佺洰褰曚簲鍒嗙被銆佸伐鍏峰璇濇璀﹀憡鎬併€佸弻涓婚閰嶈壊锛夛紱render 绾櫧鑳屾櫙鍗曟祴锛汳SI 鍗囩骇瀹夎 + 瑁呮満鍐掔儫
- 鍐呭锛氣憼 闈㈡澘婊氬姩濂戠害瀹¤淇锛堝浘灞傞潰鏉跨┖鐧藉彸閿彍鍗曟棤绌烽珮鐪熷疄缂洪櫡绛?5 澶勶級锛涒憽 涓ぎ瑙嗗浘鍋滈潬鍖猴紙鍦板浘妗嗛〉绛惧惛闄?娴姩浜掕浆/涓昏鍥炬亽鍦級+ 鐢诲竷绾櫧锛圧enderOptions.background 瑕嗙洊锛宺ender 鍗曟祴鍥涜鍍忕礌锛夛紱鈶?symbology.rs 绗﹀彿鍖栵紙鍗曡壊/鍞竴鍊?鍒嗙骇涓夋柟寮忎笁鑹插甫锛? 鎸夊眰娓叉煋鍙犲浘 + Contents 鍒嗙被灞曞紑琛?+ 鍥惧眰灞炴€ч〉锛堝父瑙?婧?瀛楁/绗﹀彿鍖栵級锛屽叆 .kyu锛涒懀 catalog.rs 宸ョ▼鐩綍浜斿垎绫伙紙鍦板浘妗?甯冨眬妗?鏁版嵁搴?鏈嶅姟閾炬帴/鏈満鏁版嵁锛?kyu 鍙屽嚮淇涓烘墦寮€宸ョ▼锛夛紱鈶?宸ュ叿鍙傛暟绫诲瀷瀵归綈 ArcGIS Python 宸ュ叿绠辫鑼冿紙澶氬€煎浘灞?鏁存暟/甯冨皵/鍧愭爣绯?绾挎€у崟浣?鑼冨洿/杈撳嚭鏂囦欢锛涜緭鍏?杈撳嚭鍒嗙粍锛涙牎楠岄敊璇?璀﹀憡/淇℃伅涓夌骇锛涚粺涓€瀵硅瘽妗嗛鏋讹紱鍚庡彴绾跨▼鎵ц + 杩涘害妯℃€佸彲鍙栨秷 + 缁堢涓夌骇鏃ュ織锛夛紱鈶?palette 璇箟鑹叉墿灞曪紙success/warning/link/accent_light/accent_strong + disabled 娲剧敓锛學CAG 娴嬭瘯鎵╁睍锛? tokens::state 鐘舵€佽壊娲剧敓鍥哄寲 + 涓婚鍒囨崲 0.2s 浜ゅ弶娣″寲 + 閫変腑 0.12s 娣″叆锛涚増鏈?0.18.0鈫?.19.0锛沝ocs 鍏ㄩ摼
- 鍋忓樊锛歡allery 鎺т欢浠嶆棤娑堣垂鍦烘櫙鏈缓锛涜繘搴︽ā鎬佷负鐬€佷互鐘舵€佹満鍗曟祴+璧版煡瑕嗙洊锛堢畻娉曞鍦ㄥ抚鍐呭畬鎴愶級
- 鍚庣画锛欰RCHITECTURE 搂9.1 浜旀潯锛堢紪杈戝唴鏍镐富绾?MCP 鏀舵暃/鎬ц兘瀹炴祴/UI 鐘舵€佹寔涔呭寲/甯冨眬妗嗕笌鏈嶅姟閾炬帴鍏戠幇锛夛紱MSI 闄?Release 寰?gh CLI

### [寮€宸 2026-08-11 kimi-code(main) 鈥?v0.19.0锛氶潰鏉挎粴鍔ㄥ姞鍥?+ 鍦板浘妗嗕腑澶惛闄?+ 鍥惧眰绗﹀彿鍖?+ 鐩綍鍒嗙被 + 閰嶈壊涓板瘜鍖?- 鑼冨洿锛歬anyu-shell锛堥潰鏉挎粴鍔?涓ぎ瑙嗗浘椤电/canvas 鎸夊眰娓叉煋/symbology/catalog/theme/ui_kit锛夈€乲anyu-render锛圧enderOptions 鑳屾櫙鍙傛暟锛夈€乨ocs 鍏ㄩ摼
- 渚濇嵁锛氱敤鎴峰叚鐐规寚浠わ紙闈㈡澘婊氬姩甯冨眬锛涘湴鍥炬鍚搁檮+绾櫧+榛樿鎵撳紑锛涘浘灞傚睘鎬э紱鍥惧眰灞曞紑绗﹀彿鍖栧垎绫伙紱鐩綍浜斿垎绫伙紱閰嶈壊涓板瘜鍖栵級锛涜鍒掓枃浠?kamala-khan-us-agent-black-canary.md
- 棰勮锛氬ぇ锛堝叚闃舵锛?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?v0.18.0锛歎I ArcGIS Pro SDK 鑼冨紡閲嶇粍 + 灞炴€ц〃 + 澶氳鍥?3D + CRS 瀹屽杽 + SDK 鎵撻€?- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?52 鍏ㄧ豢锛?12鈫?52 鍏樁娈电疮璁★級+ clippy 闆惰鍛?+ fmt 鍑€锛汸ython 鍐掔儫 22 鏂█鍏ㄨ繃锛圥ython 3.13锛?- 楠岃瘉锛氭埅鍥鹃泦鐩锛堝弻涓婚/宸ュ叿瀵硅瘽妗嗗府鍔╁尯/灞炴€ц〃/瀛楁璁＄畻鍣ㄩ瑙?3D 妫辨煴/125%路150% 缂╂斁/璺ㄥ尯鍋滈潬鍥炴祦锛夛紱MSI magic + 鍗囩骇瀹夎锛坘anyu 0.18.0 瑕嗙洊姝ｇ‘锛? 瑁呮満鎴浘鍐掔儫
- 鍐呭锛氣憼 commands.rs 澹版槑寮忓懡浠ゆ敞鍐岃〃锛?2 鍛戒护锛孌AML 鑼冨紡鎶曞奖 Ribbon/QAT/鍙抽敭鑿滃崟 + 鏉′欢缃伆锛夛紱鈶?toolbox/ 鎷嗗垎锛坧arams 鍙傛暟缁勪欢鐙珛妯″潡 + ArcGIS Pro 寮忓璇濇锛氱劍鐐瑰府鍔?鍐呰仈鏍￠獙/杩愯闂ㄦ帶/鏀惰棌/鏈€杩戜娇鐢級锛涒憿 attrcalc.rs 瀛楁璁＄畻鍣ㄨ〃杈惧紡寮曟搸锛圢ULL 浼犳挱/QGIS 璇箟锛? attrtable.rs 灞炴€ц〃锛堣櫄鎷熸粴鍔?鎺掑簭/绛涢€?瀛楁 CRUD/璁＄畻鍣ㄩ瑙堬級锛涒懀 mapview.rs 澶氬湴鍥捐鍥?+ scene3d.rs 瀹為獙 3D 妫辨煴锛堣儗闈㈠墧闄?娣卞害鎺掑簭/楂樺害瀛楁椹卞姩锛夛紱鈶?crs.rs 鍏ㄥ簱妫€绱紙7507 鏉?CrsInfo/search_crs锛岃酱搴忓疄娴?GIS 搴忎竴鑷存棤闇€淇锛?490鈫?527 卤1m 鏂█锛夛紱鈶?UI 鍥介檯瑙勮寖锛圵CAG 2.2 瀵规瘮搴﹀崟娴嬪己鍒讹細鏅ㄥ北鏈辩爞璋?0xB14E32 杈?4.79:1锛涙寚閽堢洰鏍?24px锛涚晫闈㈢缉鏀?100/125/150% 绛夋瘮瀹炶瘉锛涘仠闈犺法鍖哄洖娴佷慨澶嶏級锛涒懄 tooldef/toolrun 涓嬫矇 core锛?7 宸ュ叿涓€澶勫０鏄庯紝澹冲眰/CLI/Python 涓夐潰鎶曞奖锛夛紱鈶?kanyu-py 21鈫?8 缁戝畾 + Layer 28 閾惧紡鏂规硶 + toolbox registry 鍛戒护锛涒懆 ui_kit 鎵╀欢 menu_button/spinner/toast锛涚増鏈?0.17.0鈫?.18.0锛沝ocs 鍏ㄩ摼
- 鍋忓樊锛歡allery 鎺т欢鏈缓锛堟棤娑堣垂鍦烘櫙锛屼笉涓哄缓鑰屽缓锛夛紱4547 瀹炴祴涓?CGCS2000 CM 114E 闈炲寳浜?4锛堝凡娉ㄩ噴鏇存鎶芥祴娓呭崟锛?- 鍚庣画锛欰RCHITECTURE 搂9.1 浜旀潯锛堢紪杈戝唴鏍镐富绾?MCP 鏀舵暃鏀跺熬/鎬ц兘瀹炴祴/UI 鐘舵€佹寔涔呭寲/3D 鐪熺绾垮寲锛夛紱MSI 闄?Release 寰?gh CLI

### [寮€宸 2026-08-11 kimi-code(main) 鈥?v0.18.0锛氬伐鍏风 ArcGIS Pro 鍖?+ 灞炴€ц〃 + 澶氬湴鍥捐鍥?+ CRS 瀹屽杽 + SDK 鎵撻€?- 鑼冨洿锛歬anyu-core锛坈rs 澧炲己銆乤ttrcalc 鏂版ā鍧椼€乼ooldef 涓嬫矇锛夈€乲anyu-shell锛坈ommands 娉ㄥ唽琛?toolbox 鎷嗗垎/ui_kit 鎵╀欢/attrtable/鍦板浘澶氳鍥撅級銆乲anyu-py锛堢粦瀹氳ˉ榻愶級銆乨ocs 鍏ㄩ摼
- 渚濇嵁锛氱敤鎴峰洓鐐规寚浠わ紙宸ュ叿绠?ArcGIS Pro 鍖栫粍浠剁嫭绔嬬粍鍚堬紱鍥惧眰+灞炴€ц〃+瀛楁璁＄畻鍣紱鍦板浘瑙嗗浘绐楀彛鍖?2D/3D+鍧愭爣绯诲畬鍠勶紱鍙傜収 ArcGIS Pro SDK GitHub 鏂囨。閲嶇粍 UI 骞朵繚 SDK 鍙皟锛夛紱璁″垝鏂囦欢 she-hulk-static-red-star.md
- 棰勮锛氬ぇ锛堝叚闃舵锛?
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?v0.17.0锛歡eoprocess 绗笁鎵圭Щ妞?+ 宸ュ叿绠?37 + 鎵撳寘鍗曞叆鍙?- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?12 鍏ㄧ豢锛?99鈫?12锛宑ore 104锛? clippy 闆惰鍛?+ fmt 鍑€
- 楠岃瘉锛氬伐鍏风闈㈡澘鎴浘鐩锛?7 宸ュ叿鍒嗙被鏍戯細鐭㈤噺鍒嗘瀽 12/鐭㈤噺鍑犱綍 12/鏁版嵁绠＄悊 6锛夛紱MSI magic + msiexec /qn 闈欓粯瀹夎楠岃瘉锛堝紑濮嬭彍鍗曚粎銆屽牚鑸嗐€嶃€佹闈粎銆屽牚鑸嗐€嶃€乲anyu.exe 闅忓寘鍦ㄤ綅锛?- 鍐呭锛氣憼 geoprocess 绗笁鎵?10 绠楁硶锛坉istance_matrix+DistanceMatrix/nearest_neighbor+NearestNeighborReport/multi_ring_buffer/variable_buffer/split_by_field/add_geometry_attributes/create_grid/points_along_lines/concave_hull/minimum_rotated_rect锛実eo ConcaveHull/MinimumRotatedRect/InterpolateLine 鐩寸敤鏈檷绾э紝鍚勯厤璇箟娴嬭瘯锛夛紱鈶?toolbox.rs 鎵╃紪 27鈫?7锛圥aramKind::NumberList銆乀oolOutcome::NewLayers銆佸垱寤虹綉鏍艰寖鍥撮濉綋鍓嶆暟鎹寖鍥达級锛涒憿 鎵撳寘鍗曞叆鍙ｏ細wxs 鍘汇€屽牚鑸嗙粓绔€嶅紑濮嬭彍鍗曞揩鎹锋柟寮忥紙kanyu.exe 淇濈暀渚?MCP 鐩磋皟锛夈€丷EADME 鍚屾銆佸唴缃粓绔杩庤鍘绘棫鐗堟湰鍙凤紱鈶?鐗堟湰 0.16.0鈫?.17.0锛汳ASTERPLAN 搂6.4/ARCHITECTURE 搂2+搂9.1/CHANGELOG [0.17.0]/API.md 搂15 鍏ㄩ摼鍚屾
- 鍋忓樊锛氭棤锛堟湰鏈烘棤鏃㈡湁銆屽牚鑸嗙粓绔€嶅揩鎹锋柟寮忔畫鐣欙紝鏃犻渶娓呯悊锛?- 鍚庣画锛欰RCHITECTURE 搂9.1 浜旀潯璺嚎鎺ㄨ崘涓嶅彉锛堝睘鎬ц〃/缂栬緫鍐呮牳涓轰富绾匡級锛沜rates.io 鍙戝竷浠嶅緟 token

### [寮€宸 2026-08-11 kimi-code(main) 鈥?v0.17.0锛歡eoprocess 绗笁鎵圭Щ妞?+ 宸ュ叿绠辨墿缂?+ 鎵撳寘鍗曞叆鍙?- 鑼冨洿锛歬anyu-core geoprocess锛堢涓夋壒 10 绠楁硶锛夈€乲anyu-shell toolbox 娉ㄥ唽銆乸ackaging/wix锛堝幓銆屽牚鑸嗙粓绔€嶅揩鎹锋柟寮忥級銆乨ocs锛圡ASTERPLAN/ARCHITECTURE/CHANGELOG/README锛夈€丄I_SYNC
- 渚濇嵁锛氱敤鎴锋寚浠わ紙鎸夊姛鑳借鍒掔户缁Щ妞嶏紱鎵撳寘涓嶅嚭鐜扮嫭绔嬪牚鑸嗙粓绔紝鍏ㄩ儴闆嗘垚锛夛紱鎬昏 搂6.4 Phase 1.5锛汚RCHITECTURE 搂9.1
- 棰勮锛氬ぇ锛?0 绠楁硶 + 宸ュ叿绠?27鈫?7 + MSI 閲嶅埗涓庢湰鏈烘竻鐞嗭級

### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?shell v0.6锛氬浘灞傞潰鏉?ArcGIS Pro 鍖?+ 鍋滈潬绯荤粺 + Ribbon 鍔ㄧ敾 + QGIS 宸ュ叿绠?+ 璁剧疆缁勪欢
- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?99 鍏ㄧ豢锛?59鈫?74鈫?82鈫?99 鍥涢樁娈电疮璁★級+ clippy 闆惰鍛?+ fmt 鍑€
- 楠岃瘉锛氫節寮犳埅鍥剧洰妫€锛堝弻涓婚榛樿甯冨眬/鍒嗙粍鍦烘櫙/婕旂ず鍋滈潬锛氱粓绔诞鍔?AI 瀵硅瘽鍙抽潬+鐩綍鍏抽棴/宸ュ叿绠辨爲/璁剧疆瀵硅瘽妗?宸ュ叿鍙傛暟瀵硅瘽妗嗭級
- 鍐呭锛氣憼 Contents 鍥惧眰闈㈡澘锛坱oc.rs 绾嚱鏁扮洰褰曟爲锛氬閫夋鏄鹃殣銆佸祵濂楀垎缁勩€佺粍璺緞鍏?.kyu銆佸叏涓枃鍙抽敭鑿滃崟鍚帓搴?鍒嗙粍鎿嶄綔锛夛紱鈶?dock.rs 涓夊尯鍋滈潬绯荤粺锛堢洰褰?鍥惧眰/缁堢/AI 瀵硅瘽/宸ュ叿绠辨嫋鎷藉仠闈犮€佹诞鍔ㄧ獥銆佸叧闂?瑙嗗浘閲嶅紑锛夛紱鈶?toolbox.rs QGIS 寮忓伐鍏风锛?7 宸ュ叿 5 鍒嗙被澹版槑寮忔敞鍐岃〃 + 閫氱敤鍙傛暟琛ㄥ崟锛夛紱鈶?settings.rs 鐙珛璁剧疆锛堝潗鏍囩郴閫夋嫨 validate_crs 鏍￠獙鍏?.kyu/鐘舵€佹爮锛涙覆鏌撹缃嚜鍔熻兘鍖鸿縼鍏ワ級锛涒懁 Ribbon 鎮仠/鎸変笅/椤电涓嬪垝绾垮姩鐢伙紙tokens::animation锛夛紱鈶?geoprocess 绗簩鎵?8 绠楁硶锛坆oundary/bounding_boxes/merge/extract_by_attribute/extract_by_location/count_points_in_polygon/field_stats/mean_coordinates + FieldStats锛夛紱鈶?project.rs ProjectLayer.group 鍚戝悗鍏煎瀛楁锛涚増鏈?0.15.0鈫?.16.0锛汚RCHITECTURE/CHANGELOG/API/README/AGENTS 鍏ㄩ摼鍚屾
- 鍋忓樊锛氭棤锛坋gui 0.35 閫傞厤锛歝ontent_rect/is_decidedly_dragging锛沢eo 鏃?Boundary 鎸?OGC/QGIS 璇箟鎵嬪啓锛?- 鍚庣画锛欰RCHITECTURE 搂9.1 浜旀潯璺嚎鎺ㄨ崘锛堝睘鎬ц〃/缂栬緫鍐呮牳銆佸伐鍏风涓?MCP 鏀舵暃銆伮? 鎬ц兘瀹炴祴銆丏ockState 鎸佷箙鍖栥€佸伐鍏风鎺?AI 鎰忓浘锛夛紱crates.io 鍙戝竷浠嶅緟 token

### [寮€宸 2026-08-11 kimi-code(main) 鈥?shell v0.6锛氬浘灞傞潰鏉?ArcGIS Pro 鍖?+ 鍙仠闈犻潰鏉?+ Ribbon 鍔ㄧ敾 + QGIS 宸ュ叿绠?- 鑼冨洿锛歬anyu-shell锛坧anels/ribbon/app/ui_kit + 鏂?dock/toolbox 妯″潡锛夈€乲anyu-core geoprocess 琛ラ綈銆丄RCHITECTURE/CHANGELOG 鏂囨。鍚屾銆丟itHub 鎺ㄩ€?- 渚濇嵁锛氱敤鎴峰洓鐐规寚浠わ紙鍥惧眰鍕鹃€?鍒嗙粍/鍙抽敭鑿滃崟 ArcGIS Pro 鍖栵紱闈㈡澘鎷栧姩鍋滈潬鍏抽棴锛汻ibbon 鍥炬爣鎮仠鍔ㄧ敾锛決GIS 鏍稿績鍒嗘瀽宸ュ叿閫愪釜绉绘鎴愬伐鍏风锛涘姛鑳藉唴鍚嶇О鍏ㄩ儴涓枃锛?- 棰勮锛氬ぇ锛圲I 涓夊ぇ鍧?+ 宸ュ叿绠?+ 鏂囨。锛?
### [鏀跺伐] 2026-08-03 kimi-code(agent-5) 鈥?kanyu-shell 妗岄潰 UI MVP 钀藉湴
- 鎻愪氦锛氳鏈 commit锛涙祴璇曪細112 鍏ㄧ豢锛坈ore 65/render 15/shell-view 8/mcp 6/gene 4/闆嗘垚 14锛夛紱楠岃瘉锛歠mt/clippy 闆惰鍛?+ 鏅ㄥ北/澶滆鏄?绌虹姸鎬佷笁鎴浘鐩 + release 鍐掔儫
- 鍋忓樊锛歟frame/egui 0.35 API 澶у彉锛圫idePanel 骞跺叆 egui::Panel銆丄pp::update鈫扐pp::ui锛夊凡閫傞厤锛涗慨澶嶈嚜韬埅鍥剧姸鎬佹満 take() 璇悶鐘舵€?bug锛堝厓缁勬棤鏉′欢姹傚€艰嚧甯ф祦鏂锛夛紱release 鏋勫缓 1m42s锛堣繙蹇簬棰勪及锛夛紝kanyu-shell.exe 24.1MB
- 鍚庣画锛欸UI 鎵撳寘瀹夎 + 妗岄潰蹇嵎鏂瑰紡銆屽牚鑸嗐€嶏紙宸插悓姝?搂1.2 #1锛?
### [寮€宸 2026-08-03 kimi-code(agent-5) 鈥?kanyu-shell 妗岄潰 UI MVP 缁綔锛坅gent-3 涓€斿け鑱旀帴绠★級
- 鑼冨洿锛歝rates/kanyu-shell锛坢ain.rs/app.rs 涓讳綋锛夈€乮ntrospect銆佹枃妗ｅ悓姝ャ€佹埅鍥鹃獙璇併€乺elease 鏋勫缓
- 渚濇嵁锛氭€昏绗簩閮ㄥ垎 + 瑁佸喅 #5锛坋gui 鏂瑰悜锛夛紱鐢ㄦ埛鎸囦护锛堟闈㈢ UI 鎵撳寘瀹夎锛夛紱鎵挎帴 agent-3 鐜板満
  锛坮ender viewport 鍙傛暟銆乻hell Cargo.toml銆乿iew.rs 瑙嗗浘鏁板鍧囧凡灏辩华锛?- 棰勮锛氫腑锛堜富浣撲唬鐮?+ 楠岃瘉锛沞frame/wgpu 渚濊禆鏍戝凡鍦?Cargo.lock 瑙ｆ瀽锛?
### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?鑱斿姩鏈哄埗鏂囦欢寤虹珛
- 鎻愪氦锛氳鏈 commit锛涗緷鎹細鐢ㄦ埛鎸囦护锛圙itHub 闀夸箙鑱斿姩鏈哄埗 + 杩唬杈圭晫鍏ヨ锛?- 鍐呭锛欰I_SYNC.md 鍒濈増锛堝崗璁?蹇収/杈圭晫/浼氱绨匡級锛汚GENTS.md 鍗忚鍏ュ彛鍗囩骇

### [寮€宸 2026-08-03 kimi-code(agent-3) 鈥?kanyu-shell 妗岄潰 UI MVP
- 鑼冨洿锛歝rates/kanyu-shell锛堟柊锛夈€乲anyu-render锛坴iewport 鎵╁睍锛夈€乤ssets/銆乮ntrospect銆佹枃妗?- 渚濇嵁锛氭€昏绗簩閮ㄥ垎 + 瑁佸喅 #5锛坋gui 鏂瑰悜锛夛紱鐢ㄦ埛鎸囦护锛堟闈㈢ UI 鎵撳寘瀹夎锛?- 棰勮锛氬ぇ锛坋frame/wgpu 渚濊禆鏍戯紝release 鏋勫缓 15-25 鍒嗛挓锛?
<!-- 鏂版潯鐩姞鍦ㄨ繖琛屼箣涓?-->
### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?QGIS 鏍稿績绠楁硶绉绘 + Python 鎵撻€?+ 瀹楀湴 TXT
- 鎻愪氦锛氳鏈 commit锛涙祴璇曪細149 鍏ㄧ豢锛堝惈 parcel/toolbox 闆嗘垚娴嬭瘯锛? clippy 闆惰鍛?- 楠岃瘉锛歅ython 鐩磋皟 Rust 鍐呮牳锛坙oad/query/buffer/stats/render_png 瀹炴祴锛夛紱kanyu toolbox list/run 绔埌绔紱瀹楀湴 TXT 璇诲啓璐ㄦ寰€杩?- 鍐呭锛歡eoprocess 鍏畻娉?stats锛圦GIS 璇箟锛宬eep-first/DP/鍒犳礊闃堝€?鐐稿紑锛夛紱parcel.rs锛堝畻鍦?TXT 鍙屽悜+璐ㄦ锛屾敞鍐岃〃绗?19 鏍煎紡锛夛紱kanyu-py锛圥yO3锛?1 鍑芥暟锛? python/kanyu 鍖咃紙Layer 閾惧紡 + toolbox 杩愯鏃讹紝淇 -m 鍙屽疄渚嬪彂鐜?bug 涓?__module__ 褰掑睘鍒ゅ畾锛夛紱CLI analysis 涓冨懡浠?+ data validate + toolbox list/run锛涜鍐?#20 鍏ヨ锛涙枃妗ｅ叏閾撅紙ARCHITECTURE/API/SDK/CLI/MCP/CHANGELOG锛?- 鍋忓樊锛歞issolve 娴嬭瘯鏈熸湜鍊间慨姝ｏ紙4+4-2=6锛夛紱鍐欏嚭渚ч棴鍚堢偣澶嶇敤棣栫偣缂栧彿锛堟牸寮忔牎楠岃姹傦級
- 鍚庣画锛毬?.2 #1 鍩虹 GIS 绉绘绗竴鎵瑰畬鎴愶紱#2 crates.io 寰?token
### [寮€宸 2026-08-03 kimi-code(main) 鈥?QGIS 鏍稿績绠楁硶绉绘 + Rust/Python 鎵撻€氾紙kanyu-py + 宸ュ叿绠憋級
- 鑼冨洿锛歬anyu-core/geoprocess.rs锛圦GIS 鍏畻娉?stats锛夈€乧rates/kanyu-py锛堟柊锛孭yO3锛夈€乸ython/kanyu/锛堝寘+toolbox 绾﹀畾锛夈€丆LI toolbox 鍛戒护缁勩€佹枃妗ｏ紙瑁佸喅 #20锛?- 渚濇嵁锛氱敤鎴锋寚浠わ紙QGIS 鏍稿績绠楁硶绉绘姝ｇ‘杞啓锛汻ust 鏍稿績 Python 璋冨姩锛汚rcGIS Pro .pyt 宸ュ叿绠辨柟寮忥級锛涙€昏 搂5.1 鑴氭湰灞?- 棰勮锛氬ぇ锛堟柊 crate + Python 鍖?+ CLI + 鏂囨。锛?### [寮€宸 2026-08-03 kimi-code(main) 鈥?DWG INSERT 鎷嗗潡锛堝潡鍙傜収灞曞紑锛?- 鑼冨洿锛歝rates/kanyu-core/src/dwg.rs + 娴嬭瘯 + 鏂囨。
- 渚濇嵁锛欰I_SYNC 搂1.2 #4锛圖WG 娣卞寲锛夛紱鎬昏 搂6.4 Phase 5 閬楃暀锛泂pike 缁熻 INSERT=22.4%
- 棰勮锛氫腑锛堢害 200 琛?+ 娴嬭瘯锛?### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?椤圭洰绾у懡鍚嶄慨姝ｏ細鍩哄洜 鈫?鎶€鑳?- 鎻愪氦锛歞88820e锛涙祴璇曪細134 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛歛ttr_scaler.wasm 鎸?kanyu:skill/analyzer 鏂?ABI 閲嶇紪骞堕€氳繃 wasm-tools 缁勪欢鍖栨牎楠?- 鍐呭锛歝rate/绫诲瀷/WIT ABI/MCP 宸ュ叿锛坘anyu_skill_run/skill_list锛?CLI 鍛戒护缁勶紙kanyu skill锛?UI/鍏ㄦ枃妗ｅ悓姝ユ敼鍚嶏紱鍘嗗彶璁板綍涓嶆敼鍐欙紱AGENTS.md 渚濊禆鏂瑰悜淇锛坮ender/skill 鈫?core锛沜li/mcp/shell 鈫?core+render+skill锛?- 鍚庣画锛毬?.1 kanyu-gene 琛屽簲鏀?kanyu-skill锛堜笅杞揩鐓ч『鎵嬫洿鏂帮級
### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?妗岄潰绔?MSI 瀹夎鍖呬笌 Release 鍚屾
- 鎻愪氦锛歱ackaging/wix/kanyu.wxs锛堝畨瑁呭寘鍗充唬鐮侊級锛涢獙璇侊細MSI magic 鏍￠獙 + msiexec /qn 闈欓粯瀹夎鍏ㄦ枃浠跺氨浣嶏紱Release v0.15.0 宸查檮鍔?`kanyu-0.15.0-x86_64.msi`锛?5MB锛屽箓绛夐噸浼狅級骞舵洿鏂板畨瑁呰鏄庯紱README 蹇€熷紑濮嬪姞 MSI 璺緞
- 鍐呭锛歐iX v5锛坉otnet tool锛夊埗浣滅敤鎴风骇 MSI锛堝厤 UAC锛夛細GUI+CLI+MCP+鍥炬爣/璁稿彲锛屾闈€屽牚鑸嗐€嶄笌寮€濮嬭彍鍗曞揩鎹锋柟寮忥紱Util 鎵╁睍 PATH 鐜鍙橀噺鍥犳墿灞曡В鏋愬け璐ラ檷绾хЩ闄わ紙鏈満鎵嬪姩瀹夎宸查厤 PATH锛屼笉褰卞搷浜や粯锛?- 鍚庣画锛歳elease.yml 鍙姞 cargo-wix/MSI 宸ヤ欢锛圕I 鍖栵紝鍒楀叆 搂1.2锛?### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?shell v0.5锛氬洓鏍囧噯璁捐娣卞寲锛圚IG/QGIS/ArcGIS/閭锛?- 鎻愪氦锛氳鏈 commit 缁勶紱娴嬭瘯锛?34 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛氭祬鑹叉埅鍥句汉宸ョ‘璁わ紙QAT 涓夋寮忋€佺粍鍚嶇粍瀹藉唴灞呬腑銆丵GIS 娴忚鍣ㄦ爲鏍硅妭鐐广€丼egoe/Cascadia 瀛椾綋鏍堢敓鏁堬級
- 鍐呭锛氳仈绯婚偖绠辩粺涓€ daomingyuan@qq.com锛汚pple HIG 鏂囨湰鍒嗙骇锛?8/22/17sb/15/13/11/12锛? 杩炵画鍦嗚锛?/10/14锛? 0.5px 鍙戜笣绾?+ Segoe UI/Cascadia Code 瀛椾綋鏍堬紱鐩綍闈㈡澘鏀?QGIS 娴忚鍣ㄦ爲锛堟牴鑺傜偣鎳掑姞杞斤級锛涘浘灞?QGIS 宸ュ叿鏍?鍙抽敭鑿滃崟+绛涢€夛紱鍔熻兘鍖?QAT 涓夋寮?- 鍋忓樊锛氭棤锛坈lippy 閫傞厤锛歴ort_by_key銆乧onst assert銆侀棴鍖?mut锛?- 鍚庣画锛毬?.2 灞炴€ч潰鏉块噸寤哄緟鐢ㄦ埛瀹氬埗锛沜rates.io 鍙戝竷浠嶅緟 token
### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?shell v0.4锛歞esign-review 椹卞姩鐨勮璁¤凯浠?- 鎻愪氦锛? 涓師瀛愭彁浜わ紙Ribbon 鐗堝紡/Catalog 鍒嗙/瑙勮寖鍏ユ。锛夛紱娴嬭瘯锛?32 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛氬弻涓婚鎴浘浜哄伐纭锛圧ibbon 涓夊垎绂绘纭€佺粍鍚嶅綊浣嶃€佺洰褰曟祻瑙堝櫒鍙敤銆佹殫鑹茬晫闈?鏅ㄥ北鍦板浘锛?- 鍐呭锛歞esign-review 鎶€鑳芥矇娣€鍏?ui_kit 瑙勮寖锛汣atalog 鏂囦欢娴忚鍣紙蹇嵎浣嶇疆/闈㈠寘灞?鏁版嵁鏂囦欢杩囨护/鍙屽嚮鍔犺浇锛夛紱宸︿晶鍙岄〉绛撅紙鐩綍|鍥惧眰锛夛紱Ribbon 鐗堝紡绯荤粺淇锛堝惈缁勫悕妯法绐楀彛瀹氫綅 bug锛夛紱鍒犻櫎鍙充晶灞炴€ч潰鏉匡紙寰呯敤鎴峰畾鍒讹級
- 鍋忓樊锛氭棤锛堣鍒掑淇锛氱粍鍚嶅畾浣嶃€佸€熺敤鍐茬獊銆丮B/GB 甯搁噺浣滅敤鍩燂級
- 鍚庣画锛毬?.2 灞炴€ч潰鏉块噸寤哄緟鐢ㄦ埛瀹氬埗瑕佹眰锛沜rates.io 鍙戝竷浠嶅緟 token
### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?kanyu-shell v0.3 + KDB/KYU 鍙屾牸寮忓畾鐗?- 鎻愪氦锛氳鏈 commit锛涙祴璇曪細130 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛氬弻涓婚鎴浘浜哄伐纭锛堝浘鏍囧ぇ鎸夐挳鐗堝紡姝ｇ‘锛?*鐣岄潰澶滆鏄?+ 鍦板浘鍥哄畾鏅ㄥ北**瑙ｈ€﹀疄璇侊級锛汯DB 绔埌绔紙geojson鈫択db鈫抜nfo/query锛?- 鍐呭锛歶i_kit::icons 33 鏋氱嚎鎬у浘鏍?+ ribbon_button/tab_strip/tree_row/password_input锛汣ontents 楠ㄦ灦鐩綍锛堝純鍗＄墖寮忥級锛涘簳閮ㄥ弻椤电锛堢粓绔瘄AI 瀵硅瘽锛夛紱ai.rs 鍙岄┍鍔紙LocalDriver 鎰忓浘寮曟搸 + OpenAiDriver/ureq锛夛紱MapThemeMode 鍦板浘鑹插僵瑙ｈ€︼紱KDB锛圓rrow IPC+kanyu.*锛変笌 KYU锛圝SON 娓呭崟锛夊叆 core + 娉ㄥ唽琛ㄧ 18 鏍煎紡 + 鍏ㄦ牸寮忚浆鎹紱鏂囨。鍗囩骇锛堣鍐?#19銆伮?.5 鏍煎紡鑺傘€丄RCHITECTURE/API/README/CHANGELOG/CLI/MCP锛?- 鍋忓樊锛歳ibbon 闈欐€佺粍甯冨眬鏀?Vec锛沺ainter.arc 涓嶅瓨鍦ㄦ敼鎶樼嚎杩戜技锛況ibbon_button 鐗堝紡涓€娆′慨姝ｏ紙鎸夐挳 min_size 鎾戞弧 64脳52锛夛紱ureq 3 API 閫傞厤锛坔eader/send/read_to_string锛?- 鍚庣画锛毬?.2 寰呭姙 #2 鍥炬爣宸查棴鐜紱GUI 瀹夎涓庡揩鎹锋柟寮忔部鐢紙鏈鍚屾鏇存柊 kanyu-shell.exe锛?### [寮€宸 2026-08-03 kimi-code(main) 鈥?kanyu-shell v0.3锛欰rcGIS Pro 寮忔繁搴﹀崌绾?- 鑼冨洿锛歶i_kit锛坕cons/ribbon_button/tab_strip/tree_row/chat_bubble锛夈€乸anels锛堥鏋剁洰褰?鍙岄〉绛撅級銆乺ibbon锛堝浘鏍囨寜閽?缁勭粏鍒嗭級銆乤i.rs锛堥┍鍔?璁剧疆锛夈€乤pp锛堝湴鍥捐壊褰╄В鑰︼級銆佹枃妗?- 渚濇嵁锛氱敤鎴峰叓鐐规寚浠わ紱鎬昏 搂1.4 鍥炬爣绯荤粺/搂2.1 UI 鏋舵瀯锛涜鍒掓枃浠?hawkman-scarlet-witch-miss-martian.md
- 棰勮锛氬ぇ锛堢害 2000+ 琛屽彉鏇达級
### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?GUI 瀹夎涓庢闈㈠揩鎹锋柟寮忛棴鐜?- 鍐呭锛歬anyu-shell.exe锛圙UI 瀛愮郴缁燂紝PE 楠岃瘉锛夊畨瑁呰嚦 Programs\kanyu锛涙闈?**鍫垎.lnk**锛圙UI 鐩村惎 + 鍑ら笩鍥炬爣锛変笌 **鍫垎缁堢.lnk**锛坵t + kanyu introspect锛夊垱寤哄苟鏍￠獙锛堜腑鏂囧悕姝ｇ‘锛?- 鍋忓樊锛歅owerShell 5.1 鏃?BOM 鎸?ANSI 璇?UTF-8 鑴氭湰鑷撮杞揩鎹锋柟寮忔枃浠跺悕涔辩爜鈥斺€旀竻闄ゅ悗浠ュ甫 BOM 鑴氭湰閲嶅缓锛堟暀璁叆妗ｏ細PS 鑴氭湰涓€寰嬪甫 BOM 鎴栧叏 ASCII锛?- 鍚庣画锛毬?.2 寰呭姙 #1/#2 宸查棴鐜?### [鏀跺伐] 2026-08-03 kimi-code(main) 鈥?kanyu-shell v0.2 娣卞害 UI 鏀瑰缓钀藉湴
- 鎻愪氦锛氳鏈 commit锛涙祴璇曪細119 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛氬弻涓婚鎴浘浜哄伐纭锛圧ibbon/缁堢/闈㈡澘/瀵硅瘽妗嗛綈澶囷級
- 鍐呭锛歶i_kit 璁捐绯荤粺锛坱okens/controls/containers + 閾佸緥鍏?AGENTS.md #8锛夛紱涓冮〉绛?Ribbon锛涚嫭绔嬬粓绔紙鍐呮牳鐩撮┍锛夛紱鍙仠闈犻潰鏉匡紱11 绫诲璇濇锛涙妧鑳藉叆澹筹紱Layer::from_collection
- 鍋忓樊锛氬瓙浠ｇ悊锛坅gent-5锛夊洜閰嶉涓柇锛屼富浣撶敱涓荤嚎绋嬪畬鎴愶紱ribbon 缁勯潤鎬佸竷灞€鏀?Vec锛堥潤鎬佹彁鍗囧彈闄愶級锛涙棤鍏朵粬鍋忓樊
- 鍚庣画锛欸UI 瀹夎 + 妗岄潰蹇嵎鏂瑰紡"鍫垎"锛堟湰娆℃敹灏鹃棴鐜級
### [寮€宸 2026-08-03 kimi-code(main) 鈥?kanyu-shell v0.2 娣卞害 UI 鏀瑰缓锛圓rcGIS Pro Ribbon + 鐙珛缁堢 + bitfun 鍗＄墖瑙嗚锛?- 鑼冨洿锛歝rates/kanyu-shell 鍏ㄩ潰閲嶆瀯锛坮ibbon/panels/console/dialogs/theme 妯″潡鍖栵級銆乲anyu-skill 鎺ョ嚎鍏?shell銆佹枃妗ｄ笌浼氱绨?- 渚濇嵁锛氱敤鎴锋寚浠わ紙鍊熼壌 ArcGIS Pro 鍒嗙被璁捐 + 鐙珛缁堢 + bitfun 璁捐鎬濊矾锛夛紱鎬昏绗簩閮ㄥ垎
- 棰勮锛氬ぇ锛堢害 1500+ 琛屾柊浠ｇ爜锛?### [寮€宸 2026-08-11 kimi-code(main) 鈥?ArcGIS Pro 浣嶅浘鍥炬爣鎺ュ叆锛堟湰鏈鸿祫婧愬弻杞ㄥ埗锛?- 鑼冨洿锛歬anyu-shell ui_kit/icons.rs锛圛conCache/draw_or_image/arcgis_resource_name 鏄犲皠琛級銆乧ontrols.rs銆乺ibbon.rs銆乤pp.rs锛汚GENTS.md 鍔犲浘鏍囧伐浣滄祦
- 渚濇嵁锛氱敤鎴锋寚浠わ紙鎸?Esri DAML-ID 鍥炬爣涓嬭浇璋冪敤銆佷富棰橀鏍间紭鍖栥€佷緵鍚庣画鎵╁睍锛夛紱鎬昏 搂1.4 鍥炬爣绯荤粺
- 璁稿彲杈圭晫锛欵sri 浣嶅浘 PNG 浠呭瓨鏈満 %LOCALAPPDATA%\Programs\kanyu\icons\锛坙ight 10916 + dark 10900锛屾彁鍙栬嚜鐢ㄦ埛宸叉巿鏉?ArcGIS Pro 瀹夎锛夛紝涓嶈繘浠撳簱鍐嶅垎鍙戯紱浠撳簱淇濈暀鎵嬬粯鍥為€€
- 棰勮锛氫腑锛堢害 200 琛屽彉鏇达級
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?ArcGIS Pro 浣嶅浘鍥炬爣鎺ョ嚎闂幆
- 鎻愪氦锛?756bc2锛涙祴璇曪細149 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛氭櫒灞?澶滆鏄熷弻涓婚鎴浘鐩锛圧ibbon 澶ф寜閽?ArcGIS 褰╄壊浣嶅浘娓呮櫚銆乨ark 涓婚鍙?darkimages 鍙樹綋锛夛紱release exe 47.8MB 宸插悓姝ュ畨瑁呰嚦 Programs\kanyu
- 鍐呭锛欼conCache锛堟湰鏈?icons 鐩綍鎺㈡祴 + 涓婚绾圭悊缂撳瓨锛宼iny-skia 瑙ｇ爜锛夈€乨raw_or_image() 鍙岃建鍏ュ彛銆乤rcgis_resource_name() 33 鏋氭槧灏勮〃锛堟墿灞曠櫥璁扮偣锛夛紱ribbon_button/qat_button/Ribbon::ui 鍏ㄩ摼鎺ョ嚎锛汚GENTS.md 鍔犲浘鏍囧伐浣滄祦涓庤鍙竟鐣?- 鏈満璧勬簮锛歩cons light 10916 + dark 10900 PNG锛堟彁鍙栬嚜鐢ㄦ埛宸叉巿鏉?ArcGIS Pro 瀹夎锛屾湭鍏ヤ粨搴擄級
- 鍋忓樊锛歝lippy map_entry 鏀?entry API锛汭con::Gene 鏀瑰悕閬楁紡淇锛圫kill锛?- 鍚庣画锛氱洰褰曟爲/鍥惧眰鏍戣鍥炬爣浠嶈蛋鎵嬬粯 draw锛坱ree_row 鏈帴绾匡紝鍙悗缁瘎浼帮級锛沜rates.io 鍙戝竷浠嶅緟 token
### [鏀跺伐] 2026-08-11 kimi-code(main) 鈥?鏍戣鍥炬爣浣嶅浘鍙岃建鎺ョ嚎锛堝浘鏍囦换鍔″畬鍏ㄩ棴鐜級
- 鎻愪氦锛歞3830c0锛涙祴璇曪細149 鍏ㄧ豢 + clippy 闆惰鍛婏紱楠岃瘉锛氬弻涓婚鎴浘鐩锛堢洰褰曟爲鍘熺敓鏂囦欢澶逛綅鍥俱€乨ark 鍙樹綋姝ｇ‘锛夛紱release exe 宸插悓姝ュ畨瑁咃紙杩愯涓疄渚嬬敤鏀瑰悕鏇挎崲娉曟洿鏂帮級
- 鍐呭锛欼con 鏋氫妇 33鈫?7锛團olderPlain/Project/Database/Cad 鐩綍鏍戜笓鐢紝鎵嬬粯鍥為€€濮旀墭鏃㈡湁鐢绘硶锛夛紱tree_row/render_node/layers_tree/left_dock 鍏ㄩ摼 IconCache锛涚洰褰曡妭鐐硅涔夋牎姝ｏ紙鏂囦欢澶逛笉鍐嶇敤 folder+鍔犲彿锛?kyu/.kdb/.dwg/.dxf 鍚勬湁涓撶敤浣嶅浘锛?- 鍚庣画锛氬浘鏍囦綋绯诲畬鍏ㄩ棴鐜紙Ribbon + QAT + 鐩綍鏍?+ 鍥惧眰鏍戯級锛沜rates.io 鍙戝竷浠嶅緟 token
