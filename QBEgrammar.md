```bnf
NL := '\n'+	
BASETY := 'w' | 'l' | 's' | 'd' # Base types
EXTTY  := BASETY | 'b' | 'h'    # Extended types



 
CONST :=
    ['-'] NUMBER  # Decimal integer
  | 's_' FP       # Single-precision float
  | 'd_' FP       # Double-precision float
  | $IDENT        # Global symbol

DYNCONST :=
    CONST
  | 'thread' $IDENT          # Thread-local symbol
  | 'extern' $IDENT          # Extern symbol (GOT)
  | 'extern' 'thread' $IDENT # Extern thread-local (initial-exec)

VAL :=
    DYNCONST
  | %IDENT


LINKAGE :=
    'export' [NL]
  | 'thread' [NL]
  | 'section' SECNAME [NL]
  | 'section' SECNAME SECFLAGS [NL]

SECNAME  := '"' .... '"'
SECFLAGS := '"' .... '"'

TYPEDEF :=
    # Regular type
    'type' :IDENT '=' ['align' NUMBER]
    '{'
        ( SUBTY [NUMBER] ),
    '}'
  | # Union type
    'type' :IDENT '=' ['align' NUMBER]
    '{'
        (
            '{'
                ( SUBTY [NUMBER] ),
            '}'
        )+
    '}'
  | # Opaque type
    'type' :IDENT '=' 'align' NUMBER '{' NUMBER '}'

SUBTY := EXTTY | :IDENT


DATADEF :=
    LINKAGE*
    'data' $IDENT '=' ['align' NUMBER]
    '{'
        ( EXTTY DATAITEM+
        | 'z'   NUMBER ),
    '}'

DATAITEM :=
    $IDENT ['+' NUMBER]  # Symbol and offset
  |  '"' ... '"'         # String
  |  CONST               # Constant


FUNCDEF :=
    LINKAGE*
    'function' [ABITY] $IDENT '(' (PARAM), ')' [NL]
    '{' NL
        BLOCK+
    '}'

PARAM :=
    ABITY %IDENT  # Regular parameter
  | 'env' %IDENT  # Environment parameter (first)
  | '...'         # Variadic marker (last)

SUBWTY := 'sb' | 'ub' | 'sh' | 'uh'  # Sub-word types
ABITY  := BASETY | SUBWTY | :IDENT


BLOCK :=
    @IDENT NL     # Block label
    ( PHI NL )*   # Phi instructions
    ( INST NL )*  # Regular instructions
    JUMP NL       # Jump or return


JUMP :=
    'jmp' @IDENT               # Unconditional
  | 'jnz' VAL, @IDENT, @IDENT  # Conditional
  | 'ret' [VAL]                # Return
  | 'hlt'                      # Termination


CALL := [%IDENT '=' ABITY] 'call' VAL '(' (ARG), ')'

ARG :=
    ABITY VAL  # Regular argument
  | 'env' VAL  # Environment argument (first)
  | '...'      # Variadic marker

SUBWTY := 'sb' | 'ub' | 'sh' | 'uh'  # Sub-word types
ABITY  := BASETY | SUBWTY | :IDENT

INST :=
  
'add'
|'and'
|'div'
|'mul'
|'neg'
|'or'
|'rem'
|'sar'
|'shl'
|'shr'
|'sub'
|'udiv'
|'urem'
|'xor'

|'alloc16'
|'alloc4'
|'alloc8'
|'blit'
|'loadd'
|'loadl'
|'loads'
|'loadsb'
|'loadsh'
|'loadsw'
|'loadub'
|'loaduh'
|'loaduw'
|'loadw'
|'storeb'
|'stored'
|'storeh'
|'storel'
|'stores'
|'storew'


|'ceqd'
|'ceql'
|'ceqs'
|'ceqw'
|'cged'
|'cges'
|'cgtd'
|'cgts'
|'cled'
|'cles'
|'cltd'
|'clts'
|'cned'
|'cnel'
|'cnes'
|'cnew'
|'cod'
|'cos'
|'csgel'
|'csgew'
|'csgtl'
|'csgtw'
|'cslel'
|'cslew'
|'csltl'
|'csltw'
|'cugel'
|'cugew'
|'cugtl'
|'cugtw'
|'culel'
|'culew'
|'cultl'
|'cultw'
|'cuod'
|'cuos '


|'dtosi'
|'dtoui'
|'exts'
|'extsb'
|'extsh'
|'extsw'
|'extub'
|'extuh'
|'extuw'
|'sltof'
|'ultof'
|'stosi'
|'stoui'
|'swtof'
|'uwtof'
|'truncd'

|'cast'
|'copy'

|'call'

|'vastart'
|'vaarg'
```
