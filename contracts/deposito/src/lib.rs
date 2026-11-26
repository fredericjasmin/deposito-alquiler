#![no_std]

//! Depósito de garantía para alquileres.
//!
//! El inquilino deja el depósito en el contrato (no en la cuenta del dueño).
//! Al terminar el alquiler, una de las partes propone cuánto se le devuelve
//! al inquilino y la otra parte acepta. Recién ahí se mueve la plata:
//! lo acordado va al inquilino y el resto al propietario.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env,
};

/// Unos 30 días con ledgers de ~5 segundos.
const TTL_UMBRAL: u32 = 17_280 * 30;
/// Unos 365 días: un alquiler suele durar un año.
const TTL_EXTENDER: u32 = 17_280 * 365;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Estado {
    /// La plata está en el contrato esperando el acuerdo.
    Activo,
    /// Ya se repartió el depósito.
    Cerrado,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deposito {
    pub propietario: Address,
    pub inquilino: Address,
    pub token: Address,
    pub monto: i128,
    /// Quién hizo la última propuesta (vacío si todavía no hay).
    pub propuesta_de: Option<Address>,
    /// Cuánto del depósito vuelve al inquilino según esa propuesta.
    pub propuesta_al_inquilino: i128,
    pub estado: Estado,
}

#[contracttype]
#[derive(Clone)]
enum Clave {
    Contador,
    Deposito(u32),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    MontoInvalido = 1,
    MismaPersona = 2,
    NoExiste = 3,
    NoEsParte = 4,
    YaCerrado = 5,
    SinPropuesta = 6,
    NoPuedeAceptarSuPropia = 7,
    PropuestaCambio = 8,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositoCreado {
    #[topic]
    pub id: u32,
    pub inquilino: Address,
    pub propietario: Address,
    pub monto: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevolucionPropuesta {
    #[topic]
    pub id: u32,
    pub autor: Address,
    pub al_inquilino: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositoCerrado {
    #[topic]
    pub id: u32,
    pub al_inquilino: i128,
    pub al_propietario: i128,
}

#[contract]
pub struct DepositoAlquiler;

#[contractimpl]
impl DepositoAlquiler {
    /// El inquilino deja `monto` del `token` en el contrato.
    /// Devuelve el número del depósito.
    pub fn crear(
        env: Env,
        inquilino: Address,
        propietario: Address,
        token: Address,
        monto: i128,
    ) -> Result<u32, Error> {
        inquilino.require_auth();

        if monto <= 0 {
            return Err(Error::MontoInvalido);
        }
        if inquilino == propietario {
            return Err(Error::MismaPersona);
        }

        token::Client::new(&env, &token).transfer(
            &inquilino,
            &env.current_contract_address(),
            &monto,
        );

        let id: u32 = env
            .storage()
            .instance()
            .get(&Clave::Contador)
            .unwrap_or(0u32)
            + 1;
        env.storage().instance().set(&Clave::Contador, &id);
        env.storage()
            .instance()
            .extend_ttl(TTL_UMBRAL, TTL_EXTENDER);

        let deposito = Deposito {
            propietario: propietario.clone(),
            inquilino: inquilino.clone(),
            token,
            monto,
            propuesta_de: None,
            propuesta_al_inquilino: 0,
            estado: Estado::Activo,
        };
        guardar(&env, id, &deposito);

        DepositoCreado {
            id,
            inquilino,
            propietario,
            monto,
        }
        .publish(&env);

        Ok(id)
    }

    /// El propietario o el inquilino propone cuánto vuelve al inquilino.
    /// Una propuesta nueva reemplaza a la anterior.
    pub fn proponer(env: Env, id: u32, autor: Address, al_inquilino: i128) -> Result<(), Error> {
        autor.require_auth();

        let mut deposito = cargar(&env, id)?;
        if deposito.estado == Estado::Cerrado {
            return Err(Error::YaCerrado);
        }
        if autor != deposito.propietario && autor != deposito.inquilino {
            return Err(Error::NoEsParte);
        }
        if al_inquilino < 0 || al_inquilino > deposito.monto {
            return Err(Error::MontoInvalido);
        }

        deposito.propuesta_de = Some(autor.clone());
        deposito.propuesta_al_inquilino = al_inquilino;
        guardar(&env, id, &deposito);

        DevolucionPropuesta {
            id,
            autor,
            al_inquilino,
        }
        .publish(&env);

        Ok(())
    }

    /// La otra parte acepta la propuesta y el contrato reparte el depósito.
    /// `al_inquilino` es el monto que quien acepta vio y firmó: si la
    /// propuesta cambió mientras tanto, la llamada falla y no se mueve nada.
    pub fn aceptar(env: Env, id: u32, quien: Address, al_inquilino: i128) -> Result<(), Error> {
        quien.require_auth();

        let mut deposito = cargar(&env, id)?;
        if deposito.estado == Estado::Cerrado {
            return Err(Error::YaCerrado);
        }
        if quien != deposito.propietario && quien != deposito.inquilino {
            return Err(Error::NoEsParte);
        }
        let autor = deposito.propuesta_de.clone().ok_or(Error::SinPropuesta)?;
        if autor == quien {
            return Err(Error::NoPuedeAceptarSuPropia);
        }
        if al_inquilino != deposito.propuesta_al_inquilino {
            return Err(Error::PropuestaCambio);
        }

        // Primero se marca como cerrado y después se mueve la plata.
        deposito.estado = Estado::Cerrado;
        guardar(&env, id, &deposito);

        let al_propietario = deposito.monto - al_inquilino;
        let token = token::Client::new(&env, &deposito.token);
        let contrato = env.current_contract_address();
        if al_inquilino > 0 {
            token.transfer(&contrato, &deposito.inquilino, &al_inquilino);
        }
        if al_propietario > 0 {
            token.transfer(&contrato, &deposito.propietario, &al_propietario);
        }

        DepositoCerrado {
            id,
            al_inquilino,
            al_propietario,
        }
        .publish(&env);

        Ok(())
    }

    /// Consulta un depósito.
    pub fn ver(env: Env, id: u32) -> Result<Deposito, Error> {
        cargar(&env, id)
    }

    /// Cuántos depósitos se crearon en total.
    pub fn total(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&Clave::Contador)
            .unwrap_or(0u32)
    }
}

fn cargar(env: &Env, id: u32) -> Result<Deposito, Error> {
    env.storage()
        .persistent()
        .get(&Clave::Deposito(id))
        .ok_or(Error::NoExiste)
}

fn guardar(env: &Env, id: u32, deposito: &Deposito) {
    let clave = Clave::Deposito(id);
    env.storage().persistent().set(&clave, deposito);
    env.storage()
        .persistent()
        .extend_ttl(&clave, TTL_UMBRAL, TTL_EXTENDER);
}

mod test;
