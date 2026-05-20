/*
 * NXS conformance runner for C99.
 * Usage:
 *   cc -std=c99 -I../../nyxis-drivers/c ../../nyxis-drivers/c/nxs.c conformance/run_c.c -o run_c
 *   ./run_c conformance/
 *
 * Or from repo root:
 *   cc -std=c99 -Ic/  c/nxs.c conformance/run_c.c -o /tmp/run_c && /tmp/run_c conformance/
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <dirent.h>
#include <errno.h>

#ifdef _WIN32
#  include <direct.h>
#else
#  include <sys/stat.h>
#endif

#include "nxs.h"

/* ── Minimal JSON parser ─────────────────────────────────────────────────── */

typedef enum { JV_NULL, JV_BOOL, JV_INT, JV_FLOAT, JV_STR, JV_ARRAY, JV_OBJ } jv_type_t;

typedef struct jv jv_t;
struct jv {
    jv_type_t type;
    union {
        int       bval;
        int64_t   ival;
        double    fval;
        char     *sval;
        struct { jv_t **items; int count; } arr;
        struct { char **keys; jv_t **vals; int count; } obj;
    };
};

static jv_t *jv_null(void)  { jv_t *v = calloc(1,sizeof*v); v->type=JV_NULL;  return v; }
static jv_t *jv_bool(int b) { jv_t *v = calloc(1,sizeof*v); v->type=JV_BOOL;  v->bval=b; return v; }
static jv_t *jv_int(int64_t i){ jv_t *v=calloc(1,sizeof*v); v->type=JV_INT;  v->ival=i; return v; }
static jv_t *jv_float(double f){ jv_t *v=calloc(1,sizeof*v); v->type=JV_FLOAT;v->fval=f; return v; }
static jv_t *jv_str(const char *s, int len){
    jv_t *v=calloc(1,sizeof*v); v->type=JV_STR;
    v->sval=malloc(len+1); memcpy(v->sval,s,len); v->sval[len]=0; return v;
}
static jv_t *jv_array(void) { jv_t *v=calloc(1,sizeof*v); v->type=JV_ARRAY; return v; }
static jv_t *jv_obj(void)   { jv_t *v=calloc(1,sizeof*v); v->type=JV_OBJ;   return v; }

static void jv_arr_push(jv_t *a, jv_t *item) {
    a->arr.items = realloc(a->arr.items, (a->arr.count+1)*sizeof*a->arr.items);
    a->arr.items[a->arr.count++] = item;
}
static void jv_obj_push(jv_t *o, const char *key, int klen, jv_t *val) {
    o->obj.keys = realloc(o->obj.keys, (o->obj.count+1)*sizeof*o->obj.keys);
    o->obj.vals = realloc(o->obj.vals, (o->obj.count+1)*sizeof*o->obj.vals);
    o->obj.keys[o->obj.count] = malloc(klen+1);
    memcpy(o->obj.keys[o->obj.count], key, klen);
    o->obj.keys[o->obj.count][klen] = 0;
    o->obj.vals[o->obj.count] = val;
    o->obj.count++;
}

static jv_t *jv_obj_get(const jv_t *o, const char *key) {
    for(int i=0;i<o->obj.count;i++)
        if(strcmp(o->obj.keys[i],key)==0) return o->obj.vals[i];
    return NULL;
}

static const char *gp;

static void jskip_ws(void)   { while(*gp==' '||*gp=='\n'||*gp=='\r'||*gp=='\t') gp++; }
static jv_t *jparse(void);

static jv_t *jparse_string(void) {
    gp++; /* skip " */
    const char *start = gp;
    char buf[1<<20]; int len=0;
    while(*gp && *gp!='"') {
        if(*gp=='\\') {
            gp++;
            switch(*gp) {
                case '"':  buf[len++]='"';  gp++; break;
                case '\\': buf[len++]='\\'; gp++; break;
                case '/':  buf[len++]='/';  gp++; break;
                case 'n':  buf[len++]='\n'; gp++; break;
                case 'r':  buf[len++]='\r'; gp++; break;
                case 't':  buf[len++]='\t'; gp++; break;
                case 'u': {
                    unsigned int cp=0;
                    sscanf(gp+1,"%04x",&cp); gp+=5;
                    /* simple BMP UTF-8 */
                    if(cp<0x80) buf[len++]=(char)cp;
                    else if(cp<0x800){ buf[len++]=(char)(0xC0|(cp>>6)); buf[len++]=(char)(0x80|(cp&0x3F)); }
                    else { buf[len++]=(char)(0xE0|(cp>>12)); buf[len++]=(char)(0x80|((cp>>6)&0x3F)); buf[len++]=(char)(0x80|(cp&0x3F)); }
                    break;
                }
                default: buf[len++]=*gp++; break;
            }
        } else {
            /* handle multi-byte UTF-8 passthrough */
            unsigned char c = (unsigned char)*gp;
            if(c >= 0x80) {
                /* copy multibyte sequence as-is */
                int nb = (c>=0xF0)?4:(c>=0xE0)?3:(c>=0xC0)?2:1;
                for(int i=0;i<nb&&*gp;i++) buf[len++]=*gp++;
            } else {
                buf[len++]=*gp++;
            }
        }
    }
    if(*gp=='"') gp++;
    return jv_str(buf,len);
}

static jv_t *jparse_array(void) {
    gp++; jv_t *a=jv_array();
    for(;;) {
        jskip_ws();
        if(*gp==']') { gp++; break; }
        if(*gp==',') { gp++; continue; }
        jv_arr_push(a, jparse());
    }
    return a;
}

static jv_t *jparse_object(void) {
    gp++; jv_t *o=jv_obj();
    for(;;) {
        jskip_ws();
        if(*gp=='}') { gp++; break; }
        if(*gp==',') { gp++; continue; }
        if(*gp!='"') { gp++; continue; }
        jv_t *kv = jparse_string();
        jskip_ws(); if(*gp==':') gp++;
        jskip_ws();
        jv_t *val = jparse();
        jv_obj_push(o, kv->sval, strlen(kv->sval), val);
        free(kv->sval); free(kv);
    }
    return o;
}

static jv_t *jparse(void) {
    jskip_ws();
    char c = *gp;
    if(c=='"') return jparse_string();
    if(c=='{') return jparse_object();
    if(c=='[') return jparse_array();
    if(c=='t') { gp+=4; return jv_bool(1); }
    if(c=='f') { gp+=5; return jv_bool(0); }
    if(c=='n') { gp+=4; return jv_null(); }
    if(c=='-'||(c>='0'&&c<='9')) {
        /* detect float by scanning for . e E before parsing */
        int is_float=0;
        for(const char *p=gp;*p&&*p!=','&&*p!=']'&&*p!='}';p++)
            if(*p=='.'||*p=='e'||*p=='E') { is_float=1; break; }
        if(is_float) {
            char *end; double f=strtod(gp,&end); gp=end;
            return jv_float(f);
        } else {
            char *end; int64_t iv=strtoll(gp,&end,10); gp=end;
            return jv_int(iv);
        }
    }
    gp++;
    return jv_null();
}

static jv_t *json_parse(const char *s) {
    gp = s;
    return jparse();
}

static void jv_free(jv_t *v) {
    if(!v) return;
    if(v->type==JV_STR) free(v->sval);
    else if(v->type==JV_ARRAY){ for(int i=0;i<v->arr.count;i++) jv_free(v->arr.items[i]); free(v->arr.items); }
    else if(v->type==JV_OBJ){ for(int i=0;i<v->obj.count;i++){ free(v->obj.keys[i]); jv_free(v->obj.vals[i]); } free(v->obj.keys); free(v->obj.vals); }
    free(v);
}

/* ── File helpers ─────────────────────────────────────────────────────────── */

static uint8_t *read_file(const char *path, size_t *out_size) {
    FILE *f = fopen(path, "rb");
    if(!f) return NULL;
    fseek(f,0,SEEK_END); long sz=ftell(f); fseek(f,0,SEEK_SET);
    uint8_t *buf=malloc(sz+1);
    fread(buf,1,sz,f); buf[sz]=0;
    fclose(f);
    *out_size=(size_t)sz;
    return buf;
}

/* ── Value comparison ─────────────────────────────────────────────────────── */

static int approx_eq_d(double a, double b) {
    if(a==b) return 1;
    double diff=fabs(a-b), mag=fabs(a)>fabs(b)?fabs(a):fabs(b);
    if(mag<1e-300) return diff<1e-300;
    return diff/mag < 1e-9;
}

/* ── NXS list reading ─────────────────────────────────────────────────────── */
#define MAGIC_LIST 0x4E59584Cu

static int check_field_i64(nxs_reader_t *r, uint32_t ri, const char *key, int64_t exp) {
    nxs_object_t obj;
    if(nxs_record(r,ri,&obj)!=NXS_OK) return 0;
    int64_t v;
    if(nxs_get_i64(&obj,key,&v)!=NXS_OK) return 0;
    return v==exp;
}

static int check_field_f64(nxs_reader_t *r, uint32_t ri, const char *key, double exp) {
    nxs_object_t obj;
    if(nxs_record(r,ri,&obj)!=NXS_OK) return 0;
    double v;
    if(nxs_get_f64(&obj,key,&v)!=NXS_OK) return 0;
    return approx_eq_d(v,exp);
}

static int check_field_bool(nxs_reader_t *r, uint32_t ri, const char *key, int exp) {
    nxs_object_t obj;
    if(nxs_record(r,ri,&obj)!=NXS_OK) return 0;
    int v;
    if(nxs_get_bool(&obj,key,&v)!=NXS_OK) return 0;
    return (v!=0)==(exp!=0);
}

static int check_field_str(nxs_reader_t *r, uint32_t ri, const char *key, const char *exp) {
    nxs_object_t obj;
    if(nxs_record(r,ri,&obj)!=NXS_OK) return 0;
    char buf[256*1024];
    if(nxs_get_str(&obj,key,buf,sizeof buf)!=NXS_OK) return 0;
    return strcmp(buf,exp)==0;
}

/* ── Runner ───────────────────────────────────────────────────────────────── */

static int run_positive(const char *dir, const char *name, jv_t *exp) {
    char path[4096];
    snprintf(path,sizeof path,"%s/%s.nxb",dir,name);
    size_t sz; uint8_t *data=read_file(path,&sz);
    if(!data) { fprintf(stderr,"  cannot read %s\n",path); return 0; }

    nxs_reader_t r;
    nxs_err_t err=nxs_open(&r,data,sz);
    if(err!=NXS_OK) { free(data); fprintf(stderr,"  open failed: %d\n",err); return 0; }

    jv_t *jkeys   = jv_obj_get(exp,"keys");
    jv_t *jrecs   = jv_obj_get(exp,"records");
    jv_t *jcount  = jv_obj_get(exp,"record_count");
    int ok=1;

    /* validate record count */
    if(jcount && jcount->type==JV_INT) {
        if((uint32_t)jcount->ival != nxs_record_count(&r)) {
            fprintf(stderr,"  %s: record_count: expected %lld got %u\n",
                name,(long long)jcount->ival,nxs_record_count(&r));
            ok=0; goto done;
        }
    }

    /* validate keys */
    if(jkeys && jkeys->type==JV_ARRAY) {
        for(int i=0;i<jkeys->arr.count;i++) {
            const char *expk = jkeys->arr.items[i]->sval;
            if(i>=r.key_count||strcmp(r.keys[i],expk)!=0) {
                fprintf(stderr,"  %s: key[%d]: expected %s got %s\n",
                    name,i,expk, i<r.key_count?r.keys[i]:"(missing)");
                ok=0; goto done;
            }
        }
    }

    /* validate records */
    if(jrecs && jrecs->type==JV_ARRAY) {
        for(int ri=0;ri<jrecs->arr.count;ri++) {
            jv_t *rec = jrecs->arr.items[ri];
            if(!rec||rec->type!=JV_OBJ) continue;
            for(int fi=0;fi<rec->obj.count;fi++) {
                const char *key = rec->obj.keys[fi];
                jv_t *expv = rec->obj.vals[fi];
                if(expv->type==JV_NULL) continue; /* null/absent — skip */
                if(expv->type==JV_BOOL) {
                    if(!check_field_bool(&r,(uint32_t)ri,key,expv->bval)) {
                        fprintf(stderr,"  %s: rec[%d].%s: bool mismatch\n",name,ri,key); ok=0;
                    }
                } else if(expv->type==JV_INT) {
                    if(!check_field_i64(&r,(uint32_t)ri,key,expv->ival)) {
                        fprintf(stderr,"  %s: rec[%d].%s: i64 mismatch (exp %lld)\n",
                            name,ri,key,(long long)expv->ival); ok=0;
                    }
                } else if(expv->type==JV_FLOAT) {
                    if(!check_field_f64(&r,(uint32_t)ri,key,expv->fval)) {
                        fprintf(stderr,"  %s: rec[%d].%s: f64 mismatch (exp %g)\n",
                            name,ri,key,expv->fval); ok=0;
                    }
                } else if(expv->type==JV_STR) {
                    if(!check_field_str(&r,(uint32_t)ri,key,expv->sval)) {
                        fprintf(stderr,"  %s: rec[%d].%s: str mismatch\n",name,ri,key); ok=0;
                    }
                } else if(expv->type==JV_ARRAY) {
                    /* list field — verify via raw bytes */
                    nxs_object_t obj;
                    if(nxs_record(&r,(uint32_t)ri,&obj)!=NXS_OK) { ok=0; continue; }
                    int slot=nxs_slot(&r,key);
                    if(slot<0) { ok=0; continue; }
                    int64_t abs_off=nxs_resolve_slot(&obj,slot);
                    if(abs_off<0) { ok=0; continue; }
                    /* check list magic */
                    const uint8_t *p = data+(size_t)abs_off;
                    uint32_t magic=0; memcpy(&magic,p,4);
                    if(magic!=MAGIC_LIST) { ok=0; continue; }
                    uint8_t elem_sigil = p[8];
                    uint32_t elem_count=0; memcpy(&elem_count,p+9,4);
                    const uint8_t *dp = p+16;
                    if((int)elem_count != expv->arr.count) {
                        fprintf(stderr,"  %s: rec[%d].%s: list len %u != %d\n",
                            name,ri,key,elem_count,expv->arr.count); ok=0; continue;
                    }
                    for(int ei=0;ei<expv->arr.count;ei++) {
                        jv_t *ev = expv->arr.items[ei];
                        if(elem_sigil==0x3D && ev->type==JV_INT) {
                            int64_t got; memcpy(&got,dp+ei*8,8);
                            if(got!=ev->ival) {
                                fprintf(stderr,"  %s: rec[%d].%s[%d]: exp %lld got %lld\n",
                                    name,ri,key,ei,(long long)ev->ival,(long long)got); ok=0;
                            }
                        } else if(elem_sigil==0x7E && (ev->type==JV_FLOAT||ev->type==JV_INT)) {
                            double got; memcpy(&got,dp+ei*8,8);
                            double exp_d = (ev->type==JV_FLOAT)?ev->fval:(double)ev->ival;
                            if(!approx_eq_d(got,exp_d)) {
                                fprintf(stderr,"  %s: rec[%d].%s[%d]: f64 exp %g got %g\n",
                                    name,ri,key,ei,exp_d,got); ok=0;
                            }
                        }
                    }
                }
            }
        }
    }

done:
    nxs_close(&r);
    free(data);
    return ok;
}

static int run_negative(const char *dir, const char *name, const char *expected_code) {
    char path[4096];
    snprintf(path,sizeof path,"%s/%s.nxb",dir,name);
    size_t sz; uint8_t *data=read_file(path,&sz);
    if(!data) { fprintf(stderr,"  cannot read %s\n",path); return 0; }

    nxs_reader_t r;
    nxs_err_t err=nxs_open(&r,data,sz);
    free(data);

    if(err==NXS_OK) {
        nxs_close(&r);
        fprintf(stderr,"  %s: expected error %s but open succeeded\n",name,expected_code);
        return 0;
    }

    /* check that the error code matches */
    const char *got_code =
        err==NXS_ERR_BAD_MAGIC     ? "ERR_BAD_MAGIC"     :
        err==NXS_ERR_OUT_OF_BOUNDS ? "ERR_OUT_OF_BOUNDS" :
        err==NXS_ERR_DICT_MISMATCH ? "ERR_DICT_MISMATCH" :
        "ERR_UNKNOWN";

    if(strcmp(got_code,expected_code)!=0) {
        fprintf(stderr,"  %s: expected %s got %s\n",name,expected_code,got_code);
        return 0;
    }
    return 1;
}

int main(int argc, char **argv) {
    const char *dir = argc>1 ? argv[1] : ".";

    /* Collect *.expected.json filenames */
    DIR *d = opendir(dir);
    if(!d) { perror("opendir"); return 1; }

    char names[256][256]; int nnames=0;
    struct dirent *ent;
    while((ent=readdir(d))!=NULL && nnames<256) {
        const char *nm=ent->d_name;
        int nmlen=(int)strlen(nm);
        if(nmlen>14 && strcmp(nm+nmlen-14,".expected.json")==0) {
            int baselen=nmlen-14;
            memcpy(names[nnames],nm,baselen); names[nnames][baselen]=0;
            nnames++;
        }
    }
    closedir(d);

    /* simple sort */
    for(int i=0;i<nnames-1;i++)
        for(int j=i+1;j<nnames;j++)
            if(strcmp(names[i],names[j])>0){
                char tmp[256]; memcpy(tmp,names[i],256); memcpy(names[i],names[j],256); memcpy(names[j],tmp,256);
            }

    int passed=0, failed=0;

    for(int i=0;i<nnames;i++) {
        char jpath[4096];
        snprintf(jpath,sizeof jpath,"%s/%s.expected.json",dir,names[i]);
        size_t jsz; uint8_t *jdata=read_file(jpath,&jsz);
        if(!jdata) { fprintf(stderr,"  cannot read %s\n",jpath); failed++; continue; }

        jv_t *exp=json_parse((const char*)jdata);
        free(jdata);

        int is_neg=0;
        const char *err_code="";
        if(exp && exp->type==JV_OBJ) {
            jv_t *je=jv_obj_get(exp,"error");
            if(je && je->type==JV_STR) { is_neg=1; err_code=je->sval; }
        }

        int ok;
        if(is_neg) ok=run_negative(dir,names[i],err_code);
        else        ok=run_positive(dir,names[i],exp);

        if(ok) { printf("  PASS  %s\n",names[i]); passed++; }
        else   { fprintf(stderr,"  FAIL  %s\n",names[i]); failed++; }

        jv_free(exp);
    }

    printf("\n%d passed, %d failed\n",passed,failed);
    return failed>0 ? 1 : 0;
}
