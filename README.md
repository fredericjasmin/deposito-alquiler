# Depósito de alquiler en Stellar

Un contrato inteligente en Soroban para que el depósito de garantía de un alquiler no quede en manos de nadie hasta que dueño e inquilino se pongan de acuerdo.

Tarea 1 del Campamento 02 del Pura Vida Bootcamp: primer contrato inteligente en Stellar.

## El problema

En Costa Rica, cuando alquilás una casa o un apartamento, casi siempre te piden un depósito de garantía (normalmente un mes de alquiler). Ese dinero va directo a la cuenta del propietario.

Al irte, el propietario decide cuánto te devuelve y cuándo. Si dice que hubo daños o simplemente tarda meses en devolverlo, el inquilino no tiene mucho que hacer: la plata ya está del otro lado y reclamar por la vía legal cuesta más que el depósito.

**Lo que resuelve este contrato:** el depósito queda guardado en el contrato, no en la cuenta del propietario. Ninguno de los dos puede sacarlo solo. Para liberarlo, uno propone cuánto vuelve al inquilino y el otro tiene que aceptar con su firma. Recién ahí el contrato reparte la plata.

## Por qué elegí este problema

- Es algo que le pasa a mucha gente que conozco, sobre todo a estudiantes y a quienes se mudan seguido.
- Es chico y concreto: dos personas, un monto y un acuerdo.
- Es un escrow simple, que es justamente lo que vimos con Trustless Work: guardar fondos y liberarlos solo cuando se cumple una condición.
- Sirve para practicar lo central de Soroban: estado guardado en el contrato, autorización con `require_auth` y movimiento de tokens.

## Cómo funciona

```
Inquilino                     Contrato                      Propietario
    |  crear(monto)  ---------->  guarda el depósito              |
    |                             estado: Activo                  |
    |                                                             |
    |                  <-------- proponer(al_inquilino) ----------|
    |                             (o al revés: propone el inquilino)
    |  aceptar  --------------->  reparte y cierra                |
    |  <--- al_inquilino ------   estado: Cerrado  --- resto ---> |
```

1. **`crear`**: el inquilino firma y transfiere el depósito al contrato. Se guarda quién es el propietario, el token y el monto. Devuelve un número de depósito (`id`).
2. **`proponer`**: cualquiera de las dos partes propone cuánto vuelve al inquilino (de 0 al total). Si la otra parte no está de acuerdo, puede hacer su propia propuesta, que reemplaza a la anterior.
3. **`aceptar`**: la parte que no hizo la última propuesta la acepta, indicando el monto que está aceptando. El contrato manda lo acordado al inquilino y el resto al propietario, y marca el depósito como cerrado.
4. **`ver`** y **`total`**: consultas de solo lectura.

### Reglas que cuida el contrato

| Regla | Error |
| --- | --- |
| El monto del depósito tiene que ser mayor a 0 | `MontoInvalido` (1) |
| La propuesta tiene que estar entre 0 y el total | `MontoInvalido` (1) |
| Inquilino y propietario no pueden ser la misma cuenta | `MismaPersona` (2) |
| El depósito tiene que existir | `NoExiste` (3) |
| Solo el inquilino o el propietario pueden proponer o aceptar | `NoEsParte` (4) |
| Un depósito cerrado no se puede tocar más | `YaCerrado` (5) |
| No se puede aceptar si nadie propuso nada | `SinPropuesta` (6) |
| Nadie puede aceptar su propia propuesta | `NoPuedeAceptarSuPropia` (7) |
| Al aceptar hay que indicar el monto propuesto; si la propuesta cambió justo antes, no se acepta | `PropuestaCambio` (8) |

Cada función que mueve o decide algo pide la firma (`require_auth`) de quien la llama. El contrato emite eventos `DepositoCreado`, `DevolucionPropuesta` y `DepositoCerrado`, así una app puede mostrar el historial.

Funciona con cualquier token de Stellar (XLM, USDC, etc.), porque usa la interfaz estándar de tokens.

## Estructura

```
contracts/deposito/
  src/lib.rs    el contrato
  src/test.rs   las pruebas
  Makefile
```

## Cómo correrlo

Necesitás [Rust](https://www.rust-lang.org/tools/install), el target de WASM y el [Stellar CLI](https://developers.stellar.org/docs/tools/cli/install-cli):

```bash
rustup target add wasm32v1-none
```

### Pruebas

```bash
cargo test
```

### Compilar

```bash
stellar contract build
# genera target/wasm32v1-none/release/deposito.wasm
```

### Desplegar y probar en testnet

```bash
# Dos cuentas de prueba con XLM de Friendbot
stellar keys generate inquilino --network testnet --fund
stellar keys generate propietario --network testnet --fund

# Desplegar
stellar contract deploy --wasm target/wasm32v1-none/release/deposito.wasm \
  --source propietario --network testnet --alias deposito-alquiler

# Dirección del contrato de XLM en testnet
XLM=$(stellar contract id asset --asset native --network testnet)

# 1. El inquilino deja 100 XLM (los montos van en stroops: 1 XLM = 10 000 000)
stellar contract invoke --id deposito-alquiler --source inquilino --network testnet -- \
  crear --inquilino inquilino --propietario propietario --token $XLM --monto 1000000000

# 2. El propietario propone devolver 80 XLM (descuenta 20 por daños)
stellar contract invoke --id deposito-alquiler --source propietario --network testnet -- \
  proponer --id 1 --autor propietario --al_inquilino 800000000

# 3. El inquilino acepta y el contrato reparte
stellar contract invoke --id deposito-alquiler --source inquilino --network testnet -- \
  aceptar --id 1 --quien inquilino --al_inquilino 800000000

# Consultar
stellar contract invoke --id deposito-alquiler --source inquilino --network testnet --send=no -- \
  ver --id 1
```

## Desplegado en testnet

Contrato: [`CC7B6Q5ANYIEDKIKKZOSYGT3RP67BLWGOKSWATJ3RRFC5RSA6AJNYE3U`](https://stellar.expert/explorer/testnet/contract/CC7B6Q5ANYIEDKIKKZOSYGT3RP67BLWGOKSWATJ3RRFC5RSA6AJNYE3U)

Ahí corrí el flujo completo de arriba: un depósito de 100 XLM, el propietario propuso devolver 80, el inquilino aceptó y el contrato mandó 80 XLM al inquilino y 20 al propietario. También probé aceptar con un monto distinto al propuesto y el contrato lo rechazó con el error 8. Resultado de `ver --id 1`:

```json
{
  "estado": "Cerrado",
  "monto": "1000000000",
  "propuesta_al_inquilino": "800000000",
  "propuesta_de": "GD3BPNNDR6CQ37XYMVGFV4S63YEU4XSXNHZH6BJNWLXLH3MLPQY5JKKE",
  "inquilino": "GDAB4ERELJXNB22RO2MIRHW4RRRA6TYE3RZN7OIZO26BQNOUKMAUZQJQ",
  "propietario": "GD3BPNNDR6CQ37XYMVGFV4S63YEU4XSXNHZH6BJNWLXLH3MLPQY5JKKE",
  "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
}
```

## Pruebas

Son 15 pruebas en `contracts/deposito/src/test.rs`, con un token real de prueba (Stellar Asset Contract):

| Prueba | Qué verifica |
| --- | --- |
| `crear_guarda_la_plata_en_el_contrato` | El depósito sale del inquilino y queda en el contrato, no en el propietario |
| `crear_exige_la_firma_del_inquilino` | `crear` pide la firma del inquilino |
| `aceptar_exige_la_firma_de_quien_acepta` | `aceptar` pide la firma de quien acepta |
| `devolucion_completa` | Sin daños, el inquilino recupera todo |
| `descuento_por_danos` | Se descuenta una parte y va al propietario |
| `el_inquilino_tambien_puede_proponer` | La propuesta puede venir de cualquiera de los dos |
| `una_contrapropuesta_reemplaza_la_anterior` | Se puede negociar con contrapropuestas |
| `nadie_acepta_su_propia_propuesta` | El propietario no puede proponer 0 y aceptarse solo |
| `un_tercero_no_puede_proponer_ni_aceptar` | Una cuenta ajena no puede intervenir |
| `no_se_acepta_si_la_propuesta_cambio` | Si el propietario cambia la propuesta justo antes de que el inquilino acepte, la aceptación falla y no se mueve nada |
| `no_se_acepta_sin_propuesta` | No se reparte nada sin una propuesta |
| `no_se_reparte_dos_veces` | Un depósito cerrado no se vuelve a repartir |
| `montos_invalidos` | Montos en 0, negativos o mayores al depósito |
| `deposito_inexistente` | Consultar un `id` que no existe |
| `varios_depositos_son_independientes` | Cerrar un depósito no toca a los demás |

```
test result: ok. 15 passed; 0 failed
```

## Limitaciones y próximos pasos

- Si las partes nunca se ponen de acuerdo, el dinero queda bloqueado. Una mejora sería agregar un árbitro (por ejemplo, una persona de confianza o un servicio de resolución de disputas) que pueda decidir el reparto.
- Se podría agregar una fecha límite: si el propietario no propone nada en cierto plazo después del fin del contrato, el inquilino recupera todo.
- Falta una interfaz web para que el inquilino y el propietario lo usen desde su wallet sin la terminal.
