#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction},
    token::{StellarAssetClient, TokenClient},
    Address, Env, IntoVal, Symbol,
};

struct Escenario<'a> {
    env: Env,
    contrato: DepositoAlquilerClient<'a>,
    token: TokenClient<'a>,
    inquilino: Address,
    propietario: Address,
}

/// Un inquilino con 1 000 000 unidades del token y un propietario sin saldo.
fn escenario<'a>() -> Escenario<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin);
    let token = TokenClient::new(&env, &sac.address());

    let inquilino = Address::generate(&env);
    let propietario = Address::generate(&env);
    StellarAssetClient::new(&env, &sac.address()).mint(&inquilino, &1_000_000);

    let id = env.register(DepositoAlquiler, ());
    let contrato = DepositoAlquilerClient::new(&env, &id);

    Escenario {
        env,
        contrato,
        token,
        inquilino,
        propietario,
    }
}

#[test]
fn crear_guarda_la_plata_en_el_contrato() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    assert_eq!(id, 1);
    assert_eq!(e.contrato.total(), 1);
    assert_eq!(e.token.balance(&e.inquilino), 400_000);
    assert_eq!(e.token.balance(&e.contrato.address), 600_000);
    assert_eq!(e.token.balance(&e.propietario), 0);

    let d = e.contrato.ver(&id);
    assert_eq!(d.monto, 600_000);
    assert_eq!(d.estado, Estado::Activo);
    assert_eq!(d.propuesta_de, None);
}

#[test]
fn crear_exige_la_firma_del_inquilino() {
    let e = escenario();
    e.contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    let auths = e.env.auths();
    assert_eq!(auths[0].0, e.inquilino);
    match &auths[0].1.function {
        AuthorizedFunction::Contract((contrato, funcion, _)) => {
            assert_eq!(contrato, &e.contrato.address);
            assert_eq!(funcion, &Symbol::new(&e.env, "crear"));
        }
        _ => panic!("se esperaba la firma de una llamada al contrato"),
    }
}

#[test]
fn devolucion_completa() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    e.contrato.proponer(&id, &e.propietario, &600_000);
    e.contrato.aceptar(&id, &e.inquilino, &600_000);

    assert_eq!(e.token.balance(&e.inquilino), 1_000_000);
    assert_eq!(e.token.balance(&e.propietario), 0);
    assert_eq!(e.token.balance(&e.contrato.address), 0);
    assert_eq!(e.contrato.ver(&id).estado, Estado::Cerrado);
}

#[test]
fn descuento_por_danos() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    // El propietario descuenta 150 000 por una puerta rota.
    e.contrato.proponer(&id, &e.propietario, &450_000);
    e.contrato.aceptar(&id, &e.inquilino, &450_000);

    assert_eq!(e.token.balance(&e.inquilino), 850_000);
    assert_eq!(e.token.balance(&e.propietario), 150_000);
    assert_eq!(e.token.balance(&e.contrato.address), 0);
}

#[test]
fn el_inquilino_tambien_puede_proponer() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    e.contrato.proponer(&id, &e.inquilino, &500_000);
    e.contrato.aceptar(&id, &e.propietario, &500_000);

    assert_eq!(e.token.balance(&e.inquilino), 900_000);
    assert_eq!(e.token.balance(&e.propietario), 100_000);
}

#[test]
fn una_contrapropuesta_reemplaza_la_anterior() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    e.contrato.proponer(&id, &e.propietario, &300_000);
    e.contrato.proponer(&id, &e.inquilino, &550_000);
    e.contrato.aceptar(&id, &e.propietario, &550_000);

    assert_eq!(e.token.balance(&e.inquilino), 950_000);
    assert_eq!(e.token.balance(&e.propietario), 50_000);
}

#[test]
fn aceptar_exige_la_firma_de_quien_acepta() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);
    e.contrato.proponer(&id, &e.propietario, &600_000);
    e.contrato.aceptar(&id, &e.inquilino, &600_000);

    let auths = e.env.auths();
    assert_eq!(auths[0].0, e.inquilino);
    match &auths[0].1.function {
        AuthorizedFunction::Contract((_, funcion, args)) => {
            assert_eq!(funcion, &Symbol::new(&e.env, "aceptar"));
            assert_eq!(
                args,
                &(id, e.inquilino.clone(), 600_000_i128).into_val(&e.env)
            );
        }
        _ => panic!("se esperaba la firma de una llamada al contrato"),
    }
}

#[test]
fn nadie_acepta_su_propia_propuesta() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    e.contrato.proponer(&id, &e.propietario, &0);
    let r = e.contrato.try_aceptar(&id, &e.propietario, &0);
    assert_eq!(r, Err(Ok(Error::NoPuedeAceptarSuPropia)));
    assert_eq!(e.token.balance(&e.contrato.address), 600_000);
}

#[test]
fn un_tercero_no_puede_proponer_ni_aceptar() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);
    let extrano = Address::generate(&e.env);

    assert_eq!(
        e.contrato.try_proponer(&id, &extrano, &0),
        Err(Ok(Error::NoEsParte))
    );
    e.contrato.proponer(&id, &e.propietario, &600_000);
    assert_eq!(
        e.contrato.try_aceptar(&id, &extrano, &600_000),
        Err(Ok(Error::NoEsParte))
    );
}

#[test]
fn no_se_acepta_sin_propuesta() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    assert_eq!(
        e.contrato.try_aceptar(&id, &e.inquilino, &0),
        Err(Ok(Error::SinPropuesta))
    );
}

#[test]
fn no_se_reparte_dos_veces() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);
    e.contrato.proponer(&id, &e.propietario, &600_000);
    e.contrato.aceptar(&id, &e.inquilino, &600_000);

    assert_eq!(
        e.contrato.try_aceptar(&id, &e.inquilino, &600_000),
        Err(Ok(Error::YaCerrado))
    );
    assert_eq!(
        e.contrato.try_proponer(&id, &e.propietario, &0),
        Err(Ok(Error::YaCerrado))
    );
}

#[test]
fn montos_invalidos() {
    let e = escenario();

    assert_eq!(
        e.contrato
            .try_crear(&e.inquilino, &e.propietario, &e.token.address, &0),
        Err(Ok(Error::MontoInvalido))
    );
    assert_eq!(
        e.contrato
            .try_crear(&e.inquilino, &e.inquilino, &e.token.address, &100),
        Err(Ok(Error::MismaPersona))
    );

    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);
    assert_eq!(
        e.contrato.try_proponer(&id, &e.propietario, &600_001),
        Err(Ok(Error::MontoInvalido))
    );
    assert_eq!(
        e.contrato.try_proponer(&id, &e.propietario, &-1),
        Err(Ok(Error::MontoInvalido))
    );
}

#[test]
fn deposito_inexistente() {
    let e = escenario();
    assert_eq!(e.contrato.try_ver(&99), Err(Ok(Error::NoExiste)));
}

#[test]
fn varios_depositos_son_independientes() {
    let e = escenario();
    let otro_propietario = Address::generate(&e.env);

    let a = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &300_000);
    let b = e
        .contrato
        .crear(&e.inquilino, &otro_propietario, &e.token.address, &200_000);
    assert_eq!((a, b), (1, 2));

    e.contrato.proponer(&b, &otro_propietario, &0);
    e.contrato.aceptar(&b, &e.inquilino, &0);

    assert_eq!(e.token.balance(&otro_propietario), 200_000);
    assert_eq!(e.token.balance(&e.contrato.address), 300_000);
    assert_eq!(e.contrato.ver(&a).estado, Estado::Activo);
}

#[test]
fn no_se_acepta_si_la_propuesta_cambio() {
    let e = escenario();
    let id = e
        .contrato
        .crear(&e.inquilino, &e.propietario, &e.token.address, &600_000);

    // El inquilino vio una propuesta de 600 000, pero el propietario la
    // cambia a 0 justo antes de que el inquilino acepte.
    e.contrato.proponer(&id, &e.propietario, &600_000);
    e.contrato.proponer(&id, &e.propietario, &0);

    assert_eq!(
        e.contrato.try_aceptar(&id, &e.inquilino, &600_000),
        Err(Ok(Error::PropuestaCambio))
    );
    assert_eq!(e.token.balance(&e.contrato.address), 600_000);
    assert_eq!(e.contrato.ver(&id).estado, Estado::Activo);
}
