/* Windows' icuuc.dll, which OBS's Qt6Core imports and Wine lacks; no converter is ever made. */
typedef int UErrorCode;
#define X __declspec(dllexport)
X void *ucnv_open(const char *n, UErrorCode *e) { if (e) *e = 4; return 0; }
X void ucnv_close(void *c) {}
X void ucnv_reset(void *c) {}
X void UCNV_TO_U_CALLBACK_SUBSTITUTE(void) {}
X void UCNV_FROM_U_CALLBACK_SUBSTITUTE(void) {}
X signed char ucnv_getMaxCharSize(const void *c) { return 1; }
X const char *ucnv_getName(const void *c, UErrorCode *e) { if (e) *e = 4; return 0; }
X void ucnv_getToUCallBack(const void *c, void **a, const void **b) {}
X void ucnv_getFromUCallBack(const void *c, void **a, const void **b) {}
X void ucnv_setToUCallBack(void *c, void *a, const void *b, void **d, const void **f, UErrorCode *e) { if (e) *e = 4; }
X void ucnv_setFromUCallBack(void *c, void *a, const void *b, void **d, const void **f, UErrorCode *e) { if (e) *e = 4; }
X void ucnv_cbToUWriteUChars(void *a, const void *b, int c, int d, UErrorCode *e) { if (e) *e = 4; }
X void ucnv_cbFromUWriteUChars(void *a, const void **b, const void *c, int d, UErrorCode *e) { if (e) *e = 4; }
X int ucnv_toUCountPending(const void *c, UErrorCode *e) { return 0; }
X int ucnv_fromUCountPending(const void *c, UErrorCode *e) { return 0; }
X const char *ucnv_getStandardName(const char *n, const char *s, UErrorCode *e) { if (e) *e = 4; return 0; }
X const char *ucnv_getAvailableName(int i) { return 0; }
X int ucnv_countAvailable(void) { return 0; }
X void ucnv_toUnicode(void *c, void **t, const void *tl, const char **s, const char *sl, int *o, signed char f, UErrorCode *e) { if (e) *e = 4; }
X void ucnv_fromUnicode(void *c, char **t, const char *tl, const void **s, const void *sl, int *o, signed char f, UErrorCode *e) { if (e) *e = 4; }
